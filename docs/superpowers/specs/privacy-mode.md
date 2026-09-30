# Design : mode privé / anti-détection LiteWeb

Date: 2026-09-30
Statut: backlog (pas encore implémenté)
Branche: `feat/privacy-mode`
Portée: axes 1 (sandbox par site + purge 5 min), 2 (empreinte fixe Chrome/Linux), 3 (anti-tracking ultra durci), 4 (sandbox OS bubblewrap), 5 (kill-switch VPN NordVPN). Axe 6 (chiffrement) partiel. Limites honnêtes (TLS/JA3, IP) documentées.

## Goal

Rendre LiteWeb utilisable pour naviguer **sans laisser fuiter** ce qu'un site peut récupérer passivement : cookies, stockage local, empreinte navigateur (User-Agent, WebGL, langues, plugins, fuseau), entêtes de pistage (`Referer`, `DNT`, `Sec-CH-UA`), requêtes vers des traqueurs connus. En plus :
- **Isolation stricte par site** : 1 cookie → 1 site, sandbox détruite après 5 min d'inactivité
- **Empreinte fixe** « se fondre dans la masse » (Chrome/Linux), pas de randomisation
- **Sandbox OS** qui masque machine, timezone, résolution, fontes
- **Kill-switch VPN** : pas de VPN actif → navigation bloquée

La protection est **par site** pour le stockage, **globale** pour le reste. Pas de promesse de confidentialité réseau (le VPN externe fait ce travail).

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

### Axe 1 — Boîte étanche par site + purge 5 min

**Concept.** Chaque site est isolé dans sa propre sandbox de stockage (cookies, IndexedDB, Cache, localStorage, ServiceWorker, sessionStorage). Un cookie n'appartient **qu'à un seul site** et ne peut jamais être lu ou envoyé par un autre. Au bout de 5 min sans activité, **la sandbox entière est détruite**.

**Trois règles :**

1. **Bloquer l'inutile.** Cookies tiers refusés (`CookieAcceptPolicy::OnlyFromMainDocumentDomain`). Cookies de trackers connus (listes EasyPrivacy) refusés même first-party. Seuls les cookies nécessaires au fonctionnement du site sont acceptés.
2. **Isolation stricte « 1 cookie → 1 site ».** Chaque site (origine `scheme+host+port`) a son propre répertoire de stockage. Aucun cookie n'est visible en dehors de l'onglet qui l'a créé. Cross-site = zéro. Les requêtes sortantes vers un autre domaine n'envoient jamais de cookie, même si le cookie existe pour l'origine source.
3. **Purge après 5 min d'inactivité.** Timer `privacy::cookies` tourne toutes les 30s. Pour chaque site : si aucun onglet n'a touché à son stockage depuis 5 min → suppression complète de la sandbox (cookies + IndexedDB + Cache + localStorage + ServiceWorker + sessionStorage).

**Par défaut, non-désactivable :**
- Pas de cache HTTP persistant (`WebCacheModel::DocumentViewer` ou équivalent) — jamais de `~/.cache/liteweb/http`.
- Pas d'IndexedDB persistante (clear + block).
- Pas de localStorage persistante (clear + block).
- Pas de ServiceWorker.
- Pas de sessionStorage persistante (mémoire seulement, perdue à la fermeture).
- Pas de cache DNS, pas de cache favicon, pas de cache icônes.
- Pas d'historique de navigation (sauf si l'utilisateur l'active explicitement, hors scope par défaut).

**Implémentation.**
- `WebsiteDataManager` dédié par site (ou un seul DataManager avec répertoires segregés par host).
- Hook `WebResourceResponse` pour vider le cache WebKit à la navigation entre sites.
- Timer Rust dans `privacy::cookies` qui appelle `WebsiteDataManager.clear(...)` sur les sandbox périmées.

### Axe 2 — Empreinte fixe « se fondre dans la masse »

**Concept.** Un seul profil figé, calqué sur une configuration Linux très répandue. Pas de randomisation : un fingerprint stable est indistinguable d'un vrai utilisateur, alors qu'un fingerprint qui change à chaque session est un signal fort de « navigateur truqué ».

**Stratégie : Chrome stable sur Linux x86_64.** Des millions d'utilisateurs réels ont exactement ce fingerprint. Résidu assumé : le moteur est WebKit, pas Blink, donc quelques APIs JS diffèrent — c'est le compromis le plus honnête.

**Valeurs figées :**
```
navigator.userAgent       → "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/127.0.0.0 Safari/537.36"
navigator.platform        → "Linux x86_64"
navigator.languages       → ["fr-FR", "fr", "en"]
navigator.plugins         → []
navigator.mimeTypes       → []
navigator.hardwareConcurrency → 8
navigator.deviceMemory    → 8
navigator.maxTouchPoints  → 0
screen.width × height     → 1920 × 1080
screen.availWidth × height → 1920 × 1050
screen.colorDepth         → 24
screen.pixelDepth         → 24
window.devicePixelRatio   → 1
WebGL UNMASKED_VENDOR     → "Google Inc. (NVIDIA)"
WebGL UNMASKED_RENDERER   → "ANGLE (NVIDIA, NVIDIA GeForce GTX 1060 6GB, OpenGL 4.5)"
WebGL2 VENDOR/RENDERER    → idem
WebGL MAX_TEXTURE_SIZE   → 16384
Canvas fingerprint hash   → constant (pas de bruit)
AudioContext fingerprint  → constant (pas de bruit)
Intl.DateTimeFormat().resolvedOptions().timeZone → "Europe/Paris"
Date.prototype.getTimezoneOffset → -120 (fixe)
navigator.getBattery()    → refusé (permission deny)
```

**Pas de bruit, pas de randomisation.** `Date.now()` et `performance.now()` restent réels (les perturber crée des corrélations détectables).

**Header HTTP `Accept-Language`** aligné sur `navigator.languages` (sinon incohérence détectable).

**Implémentation** : WebExtension injectée (voir Approche) qui redéfinit les getters et fige les propriétés en `Object.defineProperty(..., {configurable: false, writable: false})`.

### Axe 3 — Anti-tracking ultra durci

**Concept.** Zéro signal émis, zéro domaine tracker accepté, zéro permission accordée sans consentement explicite. Aucun réglage n'est activable par défaut — tout est `on`.

- **Tous les en-têtes de fingerprinting strippés** (pas seulement `Sec-CH-UA`) :
  - `Sec-CH-UA` / `Sec-CH-UA-Mobile` / `Sec-CH-UA-Platform` / `Sec-CH-UA-Platform-Version` / `Sec-CH-UA-Arch` / `Sec-CH-UA-Model` / `Sec-CH-UA-Full-Version` / `Sec-CH-UA-Full-Version-List` / `Sec-CH-UA-Bitness` / `Sec-CH-UA-Wow64` / `Sec-CH-Prefers-Color-Scheme` / `Sec-CH-Prefers-Reduced-Motion` / `Sec-CH-Viewport-Width` / `Sec-CH-Viewport-Height` / `Sec-CH-DPR` / `Sec-CH-Device-Memory` / `Sec-CH-Downlink` / `Sec-CH-DPR` → **tous supprimés**
  - `Sec-Fetch-*` → laisser WebKit gérer (requis pour CORS)
  - `DNT` → `1` (envoyé)
  - `Referer` → **réduit à l'origine** même same-origin (path complet = tracking déguisé). Cross-origin : pas de `Referer` du tout.
  - `Accept-Language` → aligné sur le profil figé
  - `Origin` → laisser WebKit gérer (requis pour CORS)
- **EasyPrivacy intégré, on par défaut.** `assets/privacy/easyprivacy.txt` (subset 5k hosts, MIT). Chargé au démarrage. `:setting tracker-block off` possible mais affiche un warning. Réutilise l'infra `WebResourceRequest` filtering.
- **Aucune permission sans consentement explicite.** Layer LiteWeb au-dessus de `decide-policy` :
  - **Refusées** : géoloc, notifications, caméra, micro, écran, clipboard-read, clipboard-write, USB, Bluetooth, USB, HID, sensors, MIDI, serial, Bluetooth, Idle Detection, Window Placement, Font Access, Storage Access, Local Font Access, Local Network Access.
  - **Jamais de prompt silencieux** côté WebKit — on intercepte avant et on refuse.
  - Registre SQLite `permissions (origin, api, granted, granted_at)` — l'utilisateur peut `:permission grant <origin> <api>` pour une exception.
- **Pas de cache HTTP.** `WebCacheModel::DocumentViewer` (pas de `DocumentBrowser`). Aucun fichier HTTP sur disque.
- **Pas de prefetch.** `enable-dns-prefetching = false`, `enable-offline-web-application-cache = false`, `enable-page-cache = false`.
- **Pas de hyperlink auditing.** `enable-hyperlink-auditing = false`.
- **Pas de Media Stream / WebRTC.** `enable-media-stream = false` — pas de fuite IP via STUN.
- **Pas de WebRTC ICE.** `enable-webrtc = false` (ou `set_webrtc_ip_handling_policy('disable_non_proxied_udp')` si dispo).
- **Pas de Notifications, Pas de Fullscreen, Pas de Pointer Lock** — réduire la surface d'attaque.
- **Pas de Service Worker.** `enable-offline-web-application-cache = false`.
- **Pas de Form Autofill.** `enable-form-automation = false` si dispo.
- **Pas de Spell Checking** (envoie le texte tapé au moteur).
- **Pas de Smooth Scrolling** (vecteur de timing attack).
- **Pas de WebGL2 / WebGPU** si non requis (WebGPU = surface de fuite énorme).

## Approche

### Structure code (3 nouveaux modules + modifications ciblées)

```
src/privacy/
├── mod.rs           # pub use
├── config.rs        # PrivacyConfig (tout fixe, non-désactivable par défaut)
├── cookies.rs       # SiteSandbox (1 site = 1 répertoire), purge 5 min
├── headers.rs       # strip référent, DNT, tous les Sec-CH-*
├── fingerprint.rs   # WebExtension anti-leak (manifest.json + content.js)
├── permissions.rs   # deny-all + registre SQLite
├── tracker.rs       # EasyPrivacy loader + matcher
├── network.rs       # kill-switch VPN (détection NordVPN + blocage)
└── settings.rs      # WebKitSettings durcis (cache off, prefetch off, …)
```

```
scripts/
└── liteweb-private.sh   # launcher bubblewrap (Axe 4)
```

```
assets/privacy/
├── easyprivacy.txt     # liste de trackers
├── fonts.conf          # fontconfig réduit (6 fontes)
└── webkit-hardening.json # dump des WebKitSettings appliqués
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

### Axe 4 — Sandbox OS « cacher la machine » (bubblewrap)

**Concept.** Wrapping du binaire `liteweb` dans `bubblewrap` (bwrap) avec des bind mounts factices pour masquer les infos machine, timezone, résolution. C'est **OS-level**, pas WebKit — ça complète la couche JS.

**Ce qu'on cache / simule :**
| Fuite | Fichier / env | Valeur simulée |
|---|---|---|
| CPU model, core count, flags | `/proc/cpuinfo` (bind fake) | Intel i7-8700, 8 cores, flags génériques |
| DMI (serial, product, board) | `/sys/class/dmi/id/*` (bind fake) | `To Be Filled By O.E.M.`, générique |
| Hostname | `/etc/hostname` (bind fake) | `localhost` |
| Machine-id | `/etc/machine-id` (bind fake) | vide ou fixe |
| MAC address | `/sys/class/net/*/address` (bind fake) | `00:00:00:00:00:00` ou fixe |
| Timezone | `TZ=Europe/Paris` | forcé, cohérent avec `Intl.DateTimeFormat` |
| Locale / encoding | `LANG=fr_FR.UTF-8`, `LC_ALL=C.UTF-8` | forcé |
| Résolution écran | `screen.width` + `Xephyr` si dispo | 1920x1080 (optionnel) |
| Fontes installées | `~/.config/fontconfig/fonts.conf` (bind) | 6 fontes standard seulement |
| Env variables | nettoyée dans le sandbox | minimal (PATH, HOME, DISPLAY, XDG_*) |
| `/proc/self/maps` | partiellement maské | namespace PID (optionnel) |

**Pas besoin de Xephyr** si on spoof `screen.*` en JS (axe 2) : c'est la valeur lue par `window.screen`, pas la résolution physique X11.

**Livrable :** `scripts/liteweb-private.sh` — launcher qui exec `bwrap ... /usr/bin/liteweb "$@"`. Un `.desktop` dédié `liteweb-private.desktop`.

**Limites honnêtes :** la sandbox ne change pas l'IP publique, pas le TLS fingerprint (JA3/JA4), pas le comportement réseau. C'est pour masquer la machine locale, pas le réseau.

### Axe 5 — Kill-switch VPN (NordVPN) + blocage navigation

**Concept.** Si aucun VPN actif détecté → le navigateur **bloque toute navigation HTTP/HTTPS**. C'est un kill-switch applicatif en plus du kill-switch réseau du VPN.

**Détection NordVPN (Linux) :**
- Interface réseau `nordlynx` (WireGuard) ou `tun0`/`tun1` (OpenVPN) dans `/sys/class/net/<if>/operstate == up` et `carrier == 1`.
- Optional : `nordvpn status` (parse l'état `Status: Connected`) — mais nécessite d'être dans le groupe `nordvpn` ou sudo.
- Optionnel : vérifier que la passerelle par défaut (`ip route show default`) est via l'interface `nordlynx`.
- **Tolérance** : Mullvad (`wg0`, `tun0`), ProtonVPN (`proton0`, `tun0`) sont aussi acceptés (mêmes signatures) pour ne pas dépendre d'un seul fournisseur.

**Blocage applicatif (dans `src/privacy/network.rs`) :**
- À la création de chaque `WebView` : `should_allow_navigation` vérifie l'état VPN. Si pas de VPN → `decision.ignore()` + `show_message("VPN requis — connexion bloquée")`.
- Timer de re-check toutes les 30s (VPN peut drop) → si perdu, **toutes** les tabs actives sont redirigées vers `about:blank#vpn-lost`.
- Overlay plein écran tant que le VPN n'est pas actif : « Connexion bloquée. Activez NordVPN (ou autre VPN) pour naviguer. »
- `:vpn status` dans la palette → affiche l'état détaillé (interface, IP locale, IP publique, fournisseur détecté).

**Mise en garde.** Le kill-switch **ne masque pas** l'IP — il empêche juste le navigateur de sortir sans VPN. Le vrai masquage vient du VPN lui-même (action utilisateur, pas navigateur).

### Axe 6 — Chiffrement des connexions (partiel)

- TLS 1.3, HSTS, certificate pinning sont gérés **par WebKit**. On durcit `ssl-strict` et refuse Mixed Content (déjà par défaut dans WebKit moderne).
- Forcer HTTPS via HSTS preload injecté + `Upgrade-Insecure-Requests: 1` sur chaque requête.
- **DoH (DNS over HTTPS)** : hors scope dans cette itération (nécessite un proxy DNS local). À considérer en follow-up.
- On **ne peut pas** « re-chiffrer » une connexion non chiffrée — `https-only` via HSTS est le maximum atteignable depuis le navigateur.

## Plan d'attaque (ordre)

1. **`privacy::config` + `privacy::settings`** — fondation (WebContext + WebKitSettings durcis). Aucun changement UI.
2. **`privacy::cookies`** — SiteSandbox (1 site = 1 répertoire) + purge 5 min.
3. **`privacy::headers` + `privacy::tracker`** — strip Referer/DNT/Sec-CH-* + EasyPrivacy.
4. **`privacy::permissions`** — deny-all + registre.
5. **`privacy::fingerprint`** — WebExtension anti-leak (profil figé Chrome/Linux).
6. **`privacy::network`** — kill-switch VPN (détection NordVPN + blocage navigation).
7. **`scripts/liteweb-private.sh`** — launcher bubblewrap (Axe 4).
8. **UI** — `:setting`, `:vpn status`, `:permission`, indicateurs toolbar.
9. **README EN+FR** — section « Mode privé », avertissement honnête (« ne remplace pas un VPN », limites TLS/JA3/IP).

## Critères d'acceptation (mesurables)

- `cargo test` passe.
- **Cookies** : un site ne peut jamais lire les cookies d'un autre site (test manuel : login X sur onglet A, ouvrir X sur onglet B via un lien cross-site, aucune session partagée).
- **Purge 5 min** : après 5 min sans activité, la sandbox du site est vide (IndexedDB, localStorage, cache, cookies).
- **Empreinte** : `navigator.webdriver === false`, `navigator.plugins.length === 0`, `screen.width === 1920`, WebGL vendor `Google Inc. (NVIDIA)`. Test manuel sur 3 sites de fingerprinting (amiunique, browserleaks, creepjs) → fingerprint identique à Chrome/Linux.
- **Aucun `Referer`** cross-origin, y compris same-origin path complet. Test : requête sortante n'a pas de `Referer` path.
- **`Sec-CH-*` strippés** : aucune requête sortante ne contient `Sec-CH-UA`, `Sec-CH-UA-Platform`, etc. (vérifier via devtools ou proxy mitm).
- **EasyPrivacy** charge en <100ms, bloque ≥80% des hosts d'une cover-list de 100 trackers.
- **Permissions** : aucun prompt de permission n'apparaît sans action explicite de l'utilisateur dans `:permission`.
- **VPN kill-switch** : si NordVPN n'est pas actif, **aucune requête HTTP/HTTPS** ne sort du processus (vérifier via `tcpdump` ou `ss -tnp`). Test manuel : déconnecter le VPN → toutes les tabs passent en `about:blank#vpn-lost`.
- **Sandbox OS** : `cat /proc/cpuinfo` dans le sandbox ≠ host. `date +%Z` renvoie `Europe/Paris` (TZ forcé). `fc-list | wc -l` ≤ 8.
- **Documentation** README/EN+FR à jour avec limites honnêtes (TLS fingerprint JA3, IP visible côté serveur, besoin d'un VPN externe).

## Risques

- **Compatibilité WebKitGTK 4.1** : certaines APIs (`set_custom_accept_language`, `set_time_zone_override`, `connect_web_extension`, `set_webrtc_ip_handling_policy`) peuvent varier selon la version système. **Vérifier au début du plan 1** via `pkg-config --modversion webkit2gtk-4.1` et un test d'import minimal.
- **Cassure de sites** : le profil figé + bloqueurs agressifs + permissions deny-all = risque élevé de casser des sites. Prévoir `:setting privacy-level standard|strict` (par défaut `strict`) et un `:site-exception add <host>` pour réactiver ponctuellement.
- **VPN kill-switch** : si l'utilisateur veut naviguer sans VPN (LAN, localhost, développement), il faut un bypass explicite `:vpn require off` (ou `:vpn allow-localhost`).
- **Performance** : la WebExtension `content.js` tourne sur chaque page — ~100 lignes. Le purge timer tourne toutes les 30s. Mesurer sur `benchmark_ultra.sh` avant/après.
- **Compat Linux** : `bwrap` n'est pas installé par défaut sur toutes les distros. `scripts/liteweb-private.sh` doit vérifier et afficher un message clair si absent.