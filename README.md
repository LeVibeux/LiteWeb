# LiteWeb

Navigateur web léger pour Linux, optimisé pour la consommation CPU, RAM et énergie.

## Fonctionnalités

- Navigation classique (retour, avant, recharger, onglets)
- Blocage des navigations vers des domaines publicitaires et de traçage (~50 domaines à fort impact)
- Mode économie d'énergie (Normal / Éco / Agressif) avec suspension automatique des onglets inactifs
- Historique et favoris (SQLite local)
- Raccourcis clavier + palette de commandes (`:`)

## Prérequis (Linux)

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libsqlite3-dev build-essential
```

## Compilation

```bash
cargo build --release
```

Le binaire se trouve dans `target/release/liteweb`.

## Lancement

```bash
./target/release/liteweb
```

Données utilisateur : `~/.config/liteweb/liteweb.db`

## Sécurité

- Navigation limitée à `http://`, `https://` et `about:blank` ; les schémas locaux ou actifs (`file:`, `data:`, `javascript:`) sont refusés.
- Permissions Web (caméra, micro, géolocalisation, notifications) et sélecteurs de fichiers refusés par défaut.
- Sandbox WebKit activée, erreurs TLS bloquantes et automatisation distante désactivée.
- Base SQLite en mode `0600` et répertoires de profil en mode `0700` sur Unix.

Le filtre intégré agit sur les navigations. Il ne remplace pas un bloqueur de sous-ressources complet de type EasyList.

## Raccourcis clavier

| Raccourci | Action |
|-----------|--------|
| `Ctrl+L` | Focus barre d'adresse |
| `Ctrl+T` | Nouvel onglet |
| `Ctrl+W` | Fermer onglet |
| `Ctrl+Tab` | Onglet suivant |
| `Ctrl+Shift+Tab` | Onglet précédent |
| `Ctrl+R` / `F5` | Recharger |
| `Alt+←` / `Alt+→` | Retour / Avant |
| `Ctrl+D` | Ajouter aux favoris |
| `Ctrl+Shift+E` | Changer mode énergie |
| `:` | Palette de commandes |

## Commandes

```
:open example.com
:tab 2
:suspend
:suspend-all
:eco on|off|aggressive
:bookmark list
:history
```

## Modes énergie

| Mode | Suspension après | Onglets actifs max |
|------|------------------|--------------------|
| Normal | 10 min | 20 |
| Éco | 3 min | 10 |
| Agressif | 1 min | 5 |

Les onglets suspendus libèrent leur WebView (gain RAM significatif) et se réactivent au clic.

## Architecture

- **Rust** + **WebKitGTK 4.1** + **GTK3**
- Moteur WebKit partagé avec le système (plus léger que Chromium)
- Cache désactivé, prefetch DNS désactivé, autoplay média bloqué par défaut

## Roadmap

- [ ] Téléchargements de fichiers
- [ ] Mise à jour automatique des filtres EasyList
- [ ] Port Windows (WebView2) / macOS (WKWebView)
- [ ] GTK4 + libadwaita (quand disponible sur la cible)
