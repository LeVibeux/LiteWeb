# Design : téléchargements de fichiers (dialogue d'enregistrement explicite)

Date: 2026-09-14  
Statut: backlog (pas encore implémenté)

## Goal

Permettre de télécharger un fichier **uniquement** après confirmation explicite de l'utilisateur dans un dialogue GTK « Enregistrer sous » (audit 2026-08-17, follow-up #4 : « Download UI with an explicit save dialog, still default-deny »). Tout le reste demeure default-deny : sans confirmation, rien n'est écrit sur le disque. `:download` liste les téléchargements récents.

## État actuel

| Surface | Comportement |
|---|---|
| `WebContext` (`src/browser/webview.rs`) | `connect_download_started` → `download.cancel()` — tout téléchargement annulé (fix M1 de l'audit) |
| `decide-policy` Response | MIME non supporté → `decision.ignore()` — WebKit ne peut pas démarrer un download implicite |
| `:download` / `:dl` (`src/commands/palette.rs`) | Déjà parsé → `CommandAction::DownloadList` |
| `CommandAction::DownloadList` (`src/ui/window.rs`) | `show_message` « Gestion des téléchargements — bientôt disponible. » |
| `connect_run_file_chooser` | `cancel()` — **file inputs des pages (upload)**, pas les téléchargements ; reste annulé, hors périmètre |
| `Storage` (`src/storage/db.rs`) | Tables `history` / `bookmarks` / `suspended_tabs` ; pas de table downloads |
| README | « no downloads yet » ; les downloads sont annulés ; roadmap `[ ] File downloads` |

## Command / UI surface

| Input | Action |
|---|---|
| Réponse HTTP avec MIME non supporté / `Content-Disposition: attachment` | Dialogue GTK « Enregistrer le fichier » (dossier suggéré `~/Téléchargements` XDG, nom assaini) |
| Dialogue accepté | Écriture au chemin choisi ; entrée SQLite `in_progress` → `completed` / `failed` |
| Dialogue rejeté / fermé (Échap) | `download.cancel()` — rien n'est écrit (default deny) |
| `:download` / `:dl` | Dialogue listant les 20 derniers téléchargements : nom, URL, chemin, statut |

Pas de nouveau raccourci clavier. Pas de bouton toolbar. Le fichier n'est **jamais** ouvert automatiquement après téléchargement.

## Approach

Pipeline default-deny en trois étapes, chacune pouvant annuler :

1. **Response policy** (`webview.rs`) : URI sûre (`should_allow_navigation` = allowlist http/https **et** blocker adblock) sinon `ignore()`. MIME non supporté → `decision.download()` au lieu de `ignore()` : la réponse ne **navigue jamais** et n'écrit jamais directement — elle devient une *demande* de téléchargement, qui reste default-deny en aval.
2. **`download-started`** (`window.rs`, nouveau `wire_downloads`) : re-vérifie `should_allow_navigation` sur l'URI de la requête (couvre aussi `WebView::download_uri` et tout chemin futur). Échec (URI absente, schéma non http/https, credentials, host bloqué) → `cancel()`. Sinon, connecte `decide-destination`.
3. **`decide-destination`** : nom suggéré WebKit (dérivé de `Content-Disposition` ou du chemin d'URL) assaini par `sanitize_download_filename` (anti-traversée), dialogue `FileChooserNative` Save. Rejet → `cancel()`. Acceptation → refus des chemins sous le profil LiteWeb, `set_destination(file://…)`, enregistrement SQLite, suivi `finished` / `failed`.

Le handler cancel-all de `create_web_context` est **supprimé** et remplacé par `wire_downloads`, connecté dans `BrowserWindow::new` **avant** `render_tabs` (donc avant tout chargement). Les deux changements doivent atterrir dans le même commit : un état intermédiaire sans handler laisserait le handler par défaut de WebKit écrire dans le dossier XDG sans confirmation.

## Architecture

### Nouveau module `src/browser/downloads.rs` — helpers purs, testables

```rust
pub const FALLBACK_DOWNLOAD_FILENAME: &str = "telechargement";

/// Dernier composant de chemin uniquement (aucun `/` ni `\`), sans
/// caractères de contrôle ni bidi, sans point(s) de tête (pas de fichier
/// caché), tronqué à 128 caractères. Vide → FALLBACK_DOWNLOAD_FILENAME.
pub fn sanitize_download_filename(input: &str) -> String;

/// dirs::download_dir() (XDG), sinon ~/Downloads, sinon ".".
pub fn default_download_dir() -> PathBuf;

/// true si le chemin choisi est sous ~/.config/liteweb, ~/.local/share/liteweb
/// ou ~/.cache/liteweb (profil, base SQLite, données WebKit). Le parent est
/// canonicalisé pour résister aux symlinks.
pub fn is_forbidden_download_destination(path: &Path) -> bool;
```

Le cœur `destination_is_under(path, roots)` prend les racines en paramètre pour être testé sans toucher au vrai profil. `is_bidi_control` est ré-exporté par `src/commands/mod.rs` (déjà `pub(crate)` dans `palette.rs`).

### `src/browser/webview.rs`

- Bras `PolicyDecisionType::Response` : la vérification `should_allow_navigation` reste en premier (échec → `ignore()`). Ensuite, `!response.is_mime_type_supported()` → `decision.download()` + `true` (au lieu de `ignore()`). MIME supporté → inchangé (`false`, la navigation continue).
- `create_web_context()` : le bloc `connect_download_started(|_, download| download.cancel())` est retiré ; un commentaire doc impose que `BrowserWindow::wire_downloads` soit connecté avant le premier chargement.
- Tout le reste est inchangé : sandbox, `TLSErrorsPolicy::Fail`, `CookieAcceptPolicy::NoThirdParty`, `connect_authenticate` → cancel (HTTP Basic toujours impossible, un download derrière Basic échoue), `connect_run_file_chooser` → cancel (uploads, hors périmètre).

### `src/ui/window.rs`

`wire_downloads(state)` — appelé dans `new()` juste avant `render_tabs` :

```rust
web_context.connect_download_started(move |_, download| {
    let uri = download.request().and_then(|request| request.uri());
    if !should_allow_navigation(uri.as_deref(), |candidate| blocker.check(candidate)) {
        download.cancel(); // URI absente, schéma interdit, credentials, host bloqué
        return;
    }
    download.connect_decide_destination(/* → decide_download_destination */);
});
```

`decide_download_destination(state, download, url, suggested) -> bool` retourne `true` dans **tous** les cas (destination posée ou `cancel()`) : le handler par défaut de WebKit ne s'exécute jamais.

- Clone `storage` puis **relâche l'emprunt `RefCell` avant** `dialog.run()` (boucle principale imbriquée).
- `prompt_download_destination(&filename) -> Option<PathBuf>` : `FileChooserNative` (`FileChooserAction::Save`, « Enregistrer » / « Annuler »), `set_do_overwrite_confirmation(true)`, `set_current_folder(default_download_dir())`, `set_current_name(nom assaini)`. `ResponseType::Accept` → chemin, sinon `None`.
- Chemin refusé si `is_forbidden_download_destination` → `cancel()` + message « Téléchargement refusé ».
- Sinon `set_allow_overwrite(true)` (le dialogue a déjà confirmé l'écrasement), `set_destination(glib::filename_to_uri(path))`, `storage.add_download(url, filename, path)` → id ; `connect_finished` → `mark_download(id, completed)`, `connect_failed` → `mark_download(id, failed)`.
- Aucune interaction avec `tabs` : les téléchargements tournent dans le processus réseau de WebKit, ne réveillent **aucun** onglet suspendu et restent permis en mode Ultra / éco (ce n'est pas du JS de page).
- `show_downloads(state)` remplace le message « bientôt disponible » : `recent_downloads(20)`, format `• nom — URL → chemin [statut]` via `sanitize_ui_text` / `display_navigation_url`, `show_message("Téléchargements", …)`. Statuts affichés : « en cours », « terminé », « échoué ».
- `KEYBAR_COMMAND` mentionne `:download`.

### `src/storage/db.rs` — persistance

Le `Storage` SQLite existant est l'endroit naturel (même pattern que `history`) :

```sql
CREATE TABLE IF NOT EXISTS downloads (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    url TEXT NOT NULL,
    filename TEXT NOT NULL,
    path TEXT NOT NULL,
    status TEXT NOT NULL,
    started_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_downloads_started ON downloads(started_at DESC);
```

```rust
pub struct DownloadEntry { url, filename, path, status, started_at }
pub const DOWNLOAD_STATUS_IN_PROGRESS / _COMPLETED / _FAILED: &str;

pub fn add_download(&self, url, filename, path) -> Option<i64>; // rowid, cap 200 lignes
pub fn mark_download(&self, id: i64, status: &str);
pub fn recent_downloads(&self, limit: usize) -> Vec<DownloadEntry>;
```

Requêtes paramétrées, erreurs en `eprintln!` (pattern existant). Une entrée n'est créée **qu'après** confirmation du dialogue — un dialogue rejeté ne laisse aucune trace.

## Error handling

- Dialogue rejeté / fermé → `cancel()` silencieux (default deny, pas de message).
- URI manquante, non http/https, credentials dans l'URL, host bloqué par l'adblock → `cancel()` silencieux.
- Destination sous le profil LiteWeb → `cancel()` + `show_message("Téléchargement refusé", …)`.
- `filename_to_uri` en échec (chemin non convertible) → `cancel()`.
- Échec réseau / TLS (politique `Fail` inchangée) / HTTP Basic → `connect_failed` → statut « échoué » visible dans `:download`.
- `add_download` en échec (base indisponible) → le téléchargement continue, l'historique est best effort (comme `add_history`).
- Téléchargements simultanés : dialogues séquentiels (boucles imbriquées GTK) ; pas de file d'attente UI.
- Jamais d'emprunt `RefCell` tenu pendant `dialog.run()`.

## Testing

Unitaires (`cargo test -- <filtre>`, crate bin-only) :

- `browser::downloads` : traversée (`../../etc/passwd` → `passwd`, antislash, chemin absolu), fichiers cachés (`.bashrc` → `bashrc`), vide / `...` / `///` → fallback, contrôle + bidi retirés, troncature à 128, `destination_is_under` avec racines injectées, `default_download_dir` non vide.
- `storage` : aller-retour `add_download` → `in_progress`, `mark_download` → `completed`, cap à 200 lignes.
- `commands::palette` : régression `:download` / `:dl` → `DownloadList`.

Manuel (GTK, pas de harnais headless — même limite que l'audit) : attachment réel → dialogue avec nom assaini dans `~/Téléchargements` ; Annuler → aucun fichier ; Accepter → fichier écrit + `:download` passe de « en cours » à « terminé » ; destination sous `~/.config/liteweb` refusée ; onglets 💤 non réveillés pendant le téléchargement ; Ultra télécharge toujours ; MIME supporté (page HTML) navigue normalement.

## Out of scope

- Reprise (resume) et téléchargements parallèles avec chrome UI au-delà du dialogue liste
- Torrent, FTP
- HTTP Basic (toujours annulé — un download protégé échoue)
- File chooser d'upload des pages (reste `cancel()`)
- Ouverture automatique du fichier (`xdg-open`)
- Barre de progression par item, pause / annulation en cours de route
- Notification de fin de téléchargement
- Mise à jour du README / roadmap (au moment du ship, pas dans ce plan)
