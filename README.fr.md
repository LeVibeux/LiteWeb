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

## Benchmark de consommation

### Exemple de résultats (run local, 2026-08-11)

CPU/RAM du cgroup LiteWeb + enfants WebKit. Charge : 10 pages publiques fixes + 1 onglet blank sentinelle (sauf idle = 1 onglet Google).

| Scénario | Tous les onglets suspendus | RAM avant → après | RAM économisée | CPU après |
|----------|---------------------------:|------------------:|---------------:|----------:|
| idle | — | 394 → 394 MiB | 0% | 0,07% |
| normal | 600,6 s | 1498 → 164 MiB | **89%** (−1,3 Gio) | 0,33% |
| agressif | 60,1 s | 1395 → 204 MiB | **85%** (−1,2 Gio) | 1,8% |

<p align="center">
  <img src="assets/benchmark/memory-summary.png" alt="Mémoire avant/après suspension des onglets" width="640">
</p>
<p align="center">
  <img src="assets/benchmark/memory-over-time.png" alt="Mémoire dans le temps par scénario" width="720">
</p>
<p align="center">
  <img src="assets/benchmark/cpu-over-time.png" alt="CPU dans le temps (échelle log) par scénario" width="720">
</p>

**Lecture des gains**

- Le gain principal est la **RAM** : un onglet suspendu libère sa WebView ; seul l’onglet blank actif garde un moteur vivant. D’où une RAM « après » parfois *inférieure* au baseline idle (Google encore chargé).
- **Normal** attend les 10 min d’inactivité, puis suspend les 10 pages d’un coup → plus grosse chute de mémoire, la plus « propre ».
- **Agressif** commence par la limite d’onglets actifs (~30 s, 6 onglets), puis le timeout 1 min (~60 s, 10/10) → même type d’économie, beaucoup plus tôt.
- Le **CPU** reste bas une fois suspendu ; les pics de chargement ne comptent pas dans la fenêtre « après ». Le banc mesure CPU/RAM cgroup, pas les watts à la prise, et n’attribue pas l’économie CPU à un throttling JavaScript.
- Les chiffres dépendent de la machine et du poids réel des pages ; relancer localement pour ton matériel.

### Lancer le benchmark

```bash
./scripts/benchmark_consumption.sh
./scripts/visualize_benchmark.sh benchmark-results/run-YYYYMMDD-HHMMSS   # nécessite gnuplot
```

~15 min, session graphique, profil frais par scénario. Sorties CSV + `summary.md` sous `benchmark-results/`. Pour comparer : secteur, luminosité fixe, pas d’autres navigateurs lourds.

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
