<p align="center">
  <img src="assets/liteweb-logo-grok.jpg" alt="Logo LiteWeb" width="96">
</p>

# LiteWeb

Navigateur web léger pour Linux, optimisé pour la consommation de processeur, de mémoire et d'énergie.

## Fonctionnalités

- Navigation classique : retour, avant, rechargement et onglets
- Blocage des navigations vers des domaines publicitaires et de traçage (environ 50 domaines à fort impact)
- Modes économie d'énergie (Normal / Éco / Agressif) avec suspension automatique des onglets inactifs
- Historique et favoris dans une base SQLite locale
- Raccourcis clavier et palette de commandes (`:`)

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
- Les permissions Web (caméra, microphone, géolocalisation, notifications) et les sélecteurs de fichiers sont refusés par défaut.
- La sandbox WebKit est activée, les erreurs TLS bloquent le chargement et l'automatisation distante est désactivée.
- La base SQLite utilise le mode `0600` et les répertoires de profil le mode `0700` sous Unix.

Le filtre intégré agit sur les navigations. Il ne remplace pas un bloqueur complet de sous-ressources de type EasyList.

## Raccourcis clavier

| Raccourci | Action |
|---|---|
| `Ctrl+L` | Focus sur la barre d'adresse |
| `Ctrl+T` | Nouvel onglet |
| `Ctrl+W` | Fermer l'onglet |
| `Ctrl+Tab` | Onglet suivant |
| `Ctrl+Shift+Tab` | Onglet précédent |
| `Ctrl+R` / `F5` | Recharger |
| `Alt+←` / `Alt+→` | Retour / avant |
| `Ctrl+D` | Ajouter un favori |
| `Ctrl+Shift+E` | Changer le mode énergie |
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

| Mode | Suspension après | Onglets actifs maximum |
|------|------------------|------------------------|
| Normal | 10 min | 20 |
| Éco | 3 min | 10 |
| Agressif | 1 min | 5 |

Les onglets suspendus libèrent leur WebView, ce qui réduit fortement l'usage de mémoire, et se réactivent au clic.

## Architecture

- **Rust** + **WebKitGTK 4.1** + **GTK3**
- Moteur WebKit partagé avec le système, plus léger que Chromium
- Cache désactivé, préchargement DNS désactivé et lecture automatique des médias bloquée par défaut

## Logo

Le logo a été généré avec **Grok Image** et recadré au format 1:1 pour l'icône de LiteWeb.

## Feuille de route

- [ ] Téléchargements de fichiers
- [ ] Mise à jour automatique des filtres EasyList
- [ ] Port Windows (WebView2) / macOS (WKWebView)
- [ ] GTK4 + libadwaita (lorsqu'ils seront disponibles sur la plateforme cible)
