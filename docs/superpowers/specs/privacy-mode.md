# Design : mode privé / anti-détection LiteWeb

Date: 2026-09-30
Statut: backlog (pas encore implémenté)
Branche: `feat/privacy-mode`
Portée: axes 1 (boîte étanche cookies/stockage), 2 (empreinte navigateur), 3 (anti-tracking durci). Axes 4 (VPN intégré) et 5 (chiffrement « puissant ») **hors scope** — voir `## Hors scope` plus bas.

## Goal

Rendre LiteWeb utilisable pour naviguer **sans laisser fuiter** ce qu'un site peut récupérer passivement : cookies, stockage local, empreinte navigateur (User-Agent, WebGL, langues, plugins, fuseau), entêtes de pistage (`Referer`, `DNT`, `Sec-CH-UA`), requêtes vers des traqueurs connus. La protection est **par profil** : un profil = une boîte étanche = une identité distincte. Pas de promesse de confidentialité réseau (voir Hors scope).

## Non-goals

- Anti-fingerprinting parfait (impossible depuis WebKitGTK sans forker WebKit). On vise *réduire fortement* la surface, pas l'éliminer.
- Intégrer un VPN, Tor, ou un proxy système (hors scope navigateur).
- Cacher l'IP (impossible depuis le processus navigateur ; l'IP voyage dans la pile TCP du noyau).
- Remplacer uBlock Origin : on fait du blocage **léger** (EasyPrivacy + adblock déjà présent), pas un moteur de filtrage complet.

## État actuel

| Surface | Comportement |
|---|---|
| `WebContext` (`src/browser/webview.rs`) | `WebsiteDataManager` par défaut (cache, IndexedDB, cookies dans `$XDG_DATA_HOME/webkit-…`) |
| `WebView` (`src/browser/webview.rs`) | UA système, langues système, fuseau système, viewport réel |
| `WebResourceRequest` filtering | Adblock présent (regex host, voir `src/browser/adblock.rs`) |
| `decide_policy` (`src/browser/webview.rs`) | allowlist http/https + adblock, **pas** de strip Referer / DNT |
| Profil | Un seul profil partagé — aucune isolation entre sessions |
| Cookies | Accept-policy par défaut de WebKit (probablement `NoThirdParty` ou `Always` — à vérifier) |
| Permissions (géoloc, notif, caméra, micro) | Déléguées à WebKit sans couche de revue LiteWeb |
| Stockage local (IndexedDB, Cache, localStorage, ServiceWorker) | Persistant dans le profil WebKit par défaut |

## Axes retenus

### Axe 1 — Boîte étanche pour cookies et stockage

**Concept.** Chaque profil LiteWeb possède son propre `WebsiteDataManager` : cookies, IndexedDB, Cache Storage, localStorage, ServiceWorker, sessionStorage (en mémoire) vivent dans un répertoire dédié, jetable.

- **Profils multiples.** `:profile list`, `:profile new <nom>`, `:profile use <nom>`, `:profile drop <nom>`. Profil courant = boîte étanche. Profil `--incognito` = boîte jetée à la fermeture (déjà standard dans WebKit via `WebContext.set_ephemeral` — à vérifier sur 4.1).
- **Cookie policy stricte.** `CookieAcceptPolicy::OnlyFromMainDocumentDomain` (ou `Never` en mode paranoïaque, à exposer en setting). WebKitGTK 4.1 expose `CookieManager` via `WebContext` ; on le configure à la création du contexte par profil.
- **Vider à la sortie.** Option `clear_on_quit: bool` par profil : à la fermeture, `WebsiteDataManager.clear(...)` sur toutes les catégories.
- **Nouvelles catégories à isoler.** En plus des cookies par défaut : `IndexedDB`, `LocalStorage`, `Cache`, `ServiceWorkers`, `Itp` (Intelligent Tracking Prevention, si dispo).

### Axe 2 — Empreinte navigateur (UA spoof + JS leaks)

**Concept.** Uniformiser ce que les sites voient, **et** bloquer les fuites JS qui contournent le UA spoof.

- **User-Agent custom.** Champ `:setting user-agent` (3 modes):
  - `system` — UA par défaut WebKit (= navigateur réel)
  - `recent-chrome` — UA Chrome stable récent sur Linux x86_64 (le plus compatible)
  - `random` — UA tiré d'une liste statique de 6 UA plausibles (rotation par session)
- **Langues factices.** `:setting accept-language` : `fr-FR,fr;q=0.9,en;q=0.8` (par défaut) ou custom. WebKitGTK permet de l'injecter via le header HTTP `Accept-Language` (`WebView.set_custom_accept_language` si dispo, sinon `WebResourceRequest` filter qui réécrit l'en-tête).
- **Fuseau.** `WebView.set_time_zone_override` si dispo ; sinon via `TZ=UTC/...` côté processus (déjà géré, on l'expose juste dans `:setting`).
- **Anti-leaks JS** (dans une WebExtension injectée — voir Approche):
  - `navigator.webdriver` → `false` (déjà le cas en WebView non-Selenium, mais on fige)
  - `navigator.plugins` → 0 (figé à `[]`)
  - `navigator.languages` → suit `Accept-Lang`
  - `navigator.hardwareConcurrency` / `deviceMemory` → valeurs plausibles fixes (8 / 8)
  - `screen.width/height/colorDepth` → 1920x1080x24 fixes
  - `WebGLRenderingContext.getParameter(UNMASKED_VENDOR_WEBGL)` → `Google Inc. (NVIDIA)`
  - `Intl.DateTimeFormat().resolvedOptions().timeZone` → suit `TZ` forcé
  - `Date.now()` / `performance.now()` → **pas** perturbés (le bruit timing-based est trop visible)

### Axe 3 — Anti-tracking durci

**Concept.** Moins de signaux émis, plus de domaines bloqués.

- **En-têtes HTTP strippés.** Pour chaque requête sortante, `WebResourceRequest` filter supprime / réécrit:
  - `Referer` → réduit à l'origine (scheme + host + port), sauf si same-origin
  - `DNT` → `1` si `dnt_enabled=true`, sinon retirer
  - `Sec-CH-UA` / `Sec-CH-UA-Mobile` / `Sec-CH-UA-Platform` → retirer (ne sert qu'au fingerprinting)
  - `Sec-Fetch-*` → laisser WebKit gérer (nécessaire pour CORS)
- **EasyPrivacy intégré.** Liste statique `assets/privacy/easyprivacy.txt` (subset des hosts de la liste EasyPrivacy, 5k entrées, MIT). Rechargée à `:setting tracker-block on/off`. Réutilise l'infra `WebResourceRequest` filtering déjà en place pour l'adblock.
- **Permissions deny-by-default.** Layer LiteWeb au-dessus de `decide-policy` :
  - Géoloc, notifications, caméra, micro, écran, clipboard, USB, Bluetooth, sensors, MIDI → **refusés** sauf si l'utilisateur a explicitement autorisé l'origine (`:permission grant <origin> <api>`). Stockage du registre dans SQLite.
  - Pas de prompt silencieux côté WebKit : on intercepte avant.
- **Itp** (Intelligent Tracking Prevention). WebKitGTK 4.1 expose des settings de storage blocking (`enable-site-specific-quirks`, `top-frame-only-third-party-cookie-blocking`). On les force `on` quand `privacy_mode=strict`.

## Approche

### Structure code (3 nouveaux modules + modifications ciblées)

```
src/privacy/
├── mod.rs           # pub use, PrivacyConfig chargé du store
├── config.rs        # struct PrivacyConfig (cookie_policy, ua_mode, dnt, tracker_block, accept_language, tz, …)
├── profiles.rs      # ProfileManager : liste, create, switch, drop ; éphémère vs persistant ; clear_on_quit
├── headers.rs       # strip_referer, rewrite_accept_language, strip_dnt, strip_sec_ch_ua
├── fingerprint.rs   # injecte la WebExtension anti-leak (manifest.json + content.js)
├── permissions.rs   # PermissionRegistry (SQLite), intercept des prompts WebKit
└── tracker.rs       # EasyPrivacy loader + matcher ; étend Adblock existant
```

### Persistance

Nouvelle table SQLite `profiles` (clé primaire = nom, colonnes : `cookie_policy TEXT, ua_mode TEXT, accept_language TEXT, tz TEXT, dnt INTEGER, tracker_block INTEGER, ephemeral INTEGER, clear_on_quit INTEGER, created_at INTEGER`). Le profil courant est stocké dans `settings` (clé `current_profile`). Au démarrage, on lit le profil → on configure `WebContext` en conséquence.

### WebExtension pour l'anti-leak JS

WebKitGTK 4.1 supporte `WebExtension` via `WebContext.connect_web_extension(...)`. L'extension est un petit bundle :
- `manifest.json` (MV3 minimal, `content_scripts` sur `<all_urls>`)
- `content.js` (~80 lignes) qui redéfinit les getters vus dans Axe 2

L'extension est installée à l'init du `WebContext`. Si `WebExtension` n'est pas dispo (à vérifier sur la version système), fallback : `evaluate_javascript` au `load-changed` qui injecte le même contenu — moins propre mais fonctionne.

### UI

- `:setting` (palette existante, à étendre) : sous-section « Confidentialité »
- `:profile list | new | use | drop`
- `:permission list | grant | revoke` pour le registre de permissions
- Indicateur discret dans la barre d'URL : icône cadenas (cookie mode) + icône bouclier (tracker block) + pastille « profil: X »

### Tests

- `cargo test -- privacy::headers` — chaque réécriture d'en-tête (unit, table-driven)
- `cargo test -- privacy::fingerprint` — que l'extension charge et que `navigator.webdriver` est `false` (intégration WebKit, headless possible via `WebView` sans afficher)
- `cargo test -- privacy::profiles` — create / switch / drop / ephemeral-clear
- `cargo test -- privacy::tracker` — matcheur EasyPrivacy : host, sous-domaine, commentaire `#`, wildcard
- `cargo test -- privacy::permissions` — registre CRUD + intercept prompt

## Hors scope (axes 4 et 5)

### Axe 4 — VPN / masquage d'IP — HORS SCOPE navigateur

WebKitGTK ne crée pas d'interface réseau. Le navigateur **ne peut pas** masquer l'IP. Options réalistes pour un futur distant :

- **Détection passive.** Lire `/sys/class/net/tun*/operstate` (ou `ip link`) pour savoir si une interface VPN est active. Afficher un indicateur dans la barre LiteWeb. Aucun contrôle.
- **Wrapper externe.** Lancer `pkexec nordvpn connect <country>` depuis un menu LiteWeb. **Préférer** documenter plutôt qu'automatiser : déclenchement sudo + credentials = surface d'erreur élevée pour un bénéfice UX discutable.
- **Recommandation officielle.** Utiliser un VPN système (NordVPN, Mullvad, ProtonVPN) **avant** de lancer LiteWeb. Documenter dans le README (FR + EN).

### Axe 5 — « Chiffrement puissant des connexions » — HORS SCOPE

- TLS 1.3, HSTS, certificate pinning sont gérés **par WebKit** (et le système). On peut durcir `ssl-strict` et refuser les Mixed Content (déjà par défaut dans WebKit moderne).
- DoH (DNS over HTTPS) : nécessite un proxy DNS local (dnscrypt-proxy, systemd-resolved). WebKitGTK ne peut pas forcer DoH seul. **Possible** : lancer dnscrypt-proxy en sous-processus au boot LiteWeb et pointer le `WebContext` dessus via `set_network_tethering`. À considérer dans un **second** brainstorming.
- On **ne peut pas** « re-chiffrer » une connexion non chiffrée. Forcer HTTPS (HSTS preload injecté + Upgrade Insecure Requests) reste faisable et sera traité en follow-up si l'utilisateur le demande.

## Plan d'attaque (ordre)

1. **`privacy::config` + `privacy::profiles`** — fondation (Settings + ProfileManager + SQLite). Aucun changement UI.
2. **`privacy::headers` + `privacy::tracker`** — strip Referer/DNT/Sec-CH-UA + EasyPrivacy loader.
3. **`privacy::permissions`** — deny-by-default + registre.
4. **`privacy::fingerprint`** — WebExtension anti-leak.
5. **UI** — `:setting`, `:profile`, `:permission`, indicateurs toolbar.
6. **README EN+FR** — section « Mode privé », avertissement honnête (« ne remplace pas un VPN »).

## Critères d'acceptation (doit être mesurable, pas un wishlist)

- `cargo test` passe
- Une session sur 2 profils distincts ne partage **aucun** cookie (vérif manuelle : login site X sur profil A, switch profil B, le site ne me reconnaît pas)
- `navigator.webdriver` reste `false` sur les 3 modes UA
- `easyprivacy.txt` charge en <100ms, bloque ≥80% des hosts test (cover-list)
- `:profile drop <ephemeral>` purge IndexedDB + cookies + cache avant retour prompt
- Aucun site référencé (5 sites de test) ne reçoit `Referer` cross-origin complet
- Documentation README/EN+FR à jour, mention explicite des limites

## Risques

- **Compatibilité WebKitGTK 4.1** : certaines APIs (`set_custom_accept_language`, `set_time_zone_override`, `connect_web_extension`) peuvent varier selon la version système (Debian stable vs Ubuntu). À vérifier au début du plan 2.
- **Cassure de sites** : UA spoof + bloqueurs trop agressifs = sites qui cassent. D'où l'importance du mode `system` par défaut et de la désactivation par profil.
- **Performance** : la WebExtension `content.js` s'exécute sur chaque page — ~80 lignes n'ont jamais tué personne, mais à surveiller.