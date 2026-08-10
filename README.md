# LiteWeb

Navigateur web léger pour Linux, optimisé pour la consommation CPU, RAM et énergie.

## Fonctionnalités

- Navigation classique (retour, avant, recharger, onglets)
- Blocage intégré de pubs et traqueurs (~50 domaines à fort impact)
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
