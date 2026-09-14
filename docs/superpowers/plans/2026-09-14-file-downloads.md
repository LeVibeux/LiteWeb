# File Downloads (dialogue d'enregistrement explicite) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Allow a file download **only** after the user explicitly confirms a GTK "Enregistrer le fichier" save dialog; everything else stays default-deny (no confirmation → nothing written to disk), and `:download` lists the 20 most recent downloads from SQLite.

**Architecture:** Three-stage default-deny pipeline. (1) The `Response` policy arm in `create_webview` turns unsupported-MIME responses into a download *request* via `decision.download()` instead of `ignore()`. (2) A new `BrowserWindow::wire_downloads` replaces the cancel-all `download-started` handler: it re-checks `should_allow_navigation` on the request URI and cancels on any failure. (3) `decide-destination` sanitizes the suggested filename, shows a `FileChooserNative` Save dialog, refuses destinations under the LiteWeb profile, then sets the destination and records the download in a new SQLite `downloads` table (`in_progress` → `completed` / `failed`). Pure helpers live in a new `src/browser/downloads.rs` module.

**Tech Stack:** Rust, GTK3 (`FileChooserNative`), WebKitGTK (`Download`, `ResponsePolicyDecision`), rusqlite, `dirs`.

**Spec:** `docs/superpowers/specs/2026-09-14-file-downloads-design.md`

## Global Constraints

- **Default deny everywhere:** dialog rejected/closed (Échap) → `download.cancel()`, silent, nothing written, **no SQLite row**. A downloads row is created only *after* the dialog is accepted.
- **Atomic swap:** removing the cancel-all `connect_download_started` handler from `create_web_context` and adding `wire_downloads` MUST land in the **same commit** (Task 4). An intermediate state without any handler would let WebKit's default handler write to the XDG folder without confirmation.
- `wire_downloads` is connected in `BrowserWindow::new` **before** `Self::render_tabs(state.clone(), true)` — i.e. before any page load.
- `decide_download_destination` returns `true` in **all** paths (destination set or `cancel()` called): WebKit's default handler never runs.
- Never hold a `RefCell` borrow across `dialog.run()` (nested GTK main loop): clone `storage` first, drop all borrows, then run the dialog.
- Unsupported MIME / `Content-Disposition: attachment` responses **never navigate** and never write directly — `decision.download()` only produces a request, denied downstream by default.
- Unchanged surfaces: supported-MIME responses still navigate (`false`); `connect_run_file_chooser` stays `cancel()` (page upload inputs, out of scope); `connect_authenticate` stays `cancel()` (a download behind HTTP Basic fails → status « échoué »); sandbox, `TLSErrorsPolicy::Fail`, `CookieAcceptPolicy::NoThirdParty` untouched.
- Downloads never touch `tabs`: no suspended tab is woken, downloads keep working in Ultra/éco (WebKit network process, not page JS).
- No new keyboard shortcut, no toolbar button, the file is **never** opened automatically after download.
- SQLite: parameterized queries, errors via `eprintln!` (existing pattern), `downloads` table capped at 200 rows, best-effort (a failed `add_download` does not stop the download).
- Exact French UI strings: dialog title « Enregistrer le fichier », buttons « Enregistrer » / « Annuler », refusal dialog « Téléchargement refusé », list dialog « Téléchargements », statuses « en cours » / « terminé » / « échoué ».
- Do NOT touch `README.md` or the roadmap (ship-time change, not in this plan). Do NOT edit the spec.

Note: the crate has only a `[[bin]]` target (`liteweb`), so unit tests run with `cargo test -- <filter>` — never `cargo test --lib`.

## File map

| File | Role |
|------|------|
| `src/browser/downloads.rs` | NEW — pure helpers: `sanitize_download_filename`, `default_download_dir`, `is_forbidden_download_destination`, `destination_is_under` + unit tests |
| `src/browser/mod.rs` | Declare + re-export the new module |
| `src/commands/mod.rs` | Re-export `is_bidi_control` (already `pub(crate)` in `palette.rs`) |
| `src/commands/palette.rs` | Regression tests only: `:download` / `:dl` → `DownloadList` (parsing already exists) |
| `src/storage/db.rs` | `downloads` table, `DownloadEntry`, status consts, `add_download` / `mark_download` / `recent_downloads` + unit tests |
| `src/storage/mod.rs` | Re-export the new storage items |
| `src/browser/webview.rs` | `Response` arm → `decision.download()`; remove cancel-all handler (Task 4) |
| `src/ui/window.rs` | `wire_downloads`, `decide_download_destination`, `prompt_download_destination`, `show_downloads`, `KEYBAR_COMMAND` |

---

### Task 1: `src/browser/downloads.rs` — pure, testable helpers

**Files:**
- Create: `src/browser/downloads.rs`
- Modify: `src/browser/mod.rs`
- Modify: `src/commands/mod.rs` (add `is_bidi_control` to the `pub(crate) use` list)
- Test: `src/browser/downloads.rs` (`#[cfg(test)]` module at the bottom)

**Interfaces:**
- Consumes: `crate::commands::is_bidi_control` (already `pub(crate)` in `src/commands/palette.rs:133`, just not re-exported by `src/commands/mod.rs`).
- Produces (used by Task 4):
  - `pub const FALLBACK_DOWNLOAD_FILENAME: &str = "telechargement";`
  - `pub fn sanitize_download_filename(input: &str) -> String`
  - `pub fn default_download_dir() -> PathBuf`
  - `pub fn is_forbidden_download_destination(path: &Path) -> bool`
  - private core `fn destination_is_under(path: &Path, roots: &[PathBuf]) -> bool` (takes roots as a parameter so tests never touch the real profile).

- [ ] **Step 1: Write the failing tests**

Create `src/browser/downloads.rs` with ONLY the test module for now (plus the `use` lines), so the first `cargo test` run fails to compile for the right reason:

```rust
use std::path::{Path, PathBuf};

use crate::commands::is_bidi_control;

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp_root(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "liteweb-downloads-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ))
    }

    #[test]
    fn strips_path_traversal_to_last_component() {
        assert_eq!(sanitize_download_filename("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_download_filename("/etc/passwd"), "passwd");
        assert_eq!(
            sanitize_download_filename("..\\..\\Windows\\evil.exe"),
            "evil.exe"
        );
    }

    #[test]
    fn strips_leading_dots_no_hidden_files() {
        assert_eq!(sanitize_download_filename(".bashrc"), "bashrc");
        assert_eq!(sanitize_download_filename("..config"), "config");
    }

    #[test]
    fn empty_dots_and_slashes_fall_back() {
        assert_eq!(sanitize_download_filename(""), FALLBACK_DOWNLOAD_FILENAME);
        assert_eq!(sanitize_download_filename("..."), FALLBACK_DOWNLOAD_FILENAME);
        assert_eq!(sanitize_download_filename("///"), FALLBACK_DOWNLOAD_FILENAME);
    }

    #[test]
    fn removes_control_and_bidi_characters() {
        assert_eq!(
            sanitize_download_filename("fi\u{202E}le\u{0007}\u{000A}.txt"),
            "file.txt"
        );
    }

    #[test]
    fn truncates_to_128_chars() {
        let long = "a".repeat(200);
        assert_eq!(sanitize_download_filename(&long).chars().count(), 128);
    }

    #[test]
    fn default_download_dir_is_not_empty() {
        assert!(!default_download_dir().as_os_str().is_empty());
    }

    #[test]
    fn destination_is_under_detects_profile_roots() {
        let root = unique_temp_root("roots");
        let profile = root.join("config").join("liteweb");
        std::fs::create_dir_all(&profile).unwrap();
        let elsewhere = root.join("downloads");
        std::fs::create_dir_all(&elsewhere).unwrap();

        let roots = vec![profile.clone()];
        assert!(destination_is_under(&profile.join("liteweb.db"), &roots));
        // Non-existent parent: canonicalize fails, raw-path fallback still matches.
        assert!(destination_is_under(&profile.join("sub").join("x.bin"), &roots));
        assert!(!destination_is_under(&elsewhere.join("file.bin"), &roots));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn destination_is_under_resists_symlinked_parents() {
        let root = unique_temp_root("symlink");
        let profile = root.join("liteweb");
        std::fs::create_dir_all(&profile).unwrap();
        let alias = root.join("alias");
        std::os::unix::fs::symlink(&profile, &alias).unwrap();

        let roots = vec![profile];
        // Writing through the symlinked parent must still be detected.
        assert!(destination_is_under(&alias.join("liteweb.db"), &roots));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn plain_tmp_path_is_not_forbidden() {
        assert!(!is_forbidden_download_destination(Path::new(
            "/tmp/liteweb-plain-download.bin"
        )));
    }
}
```

Declare the module. In `src/browser/mod.rs`, the file currently starts with:

```rust
mod reader;
mod tab;
mod tab_manager;
mod webview;
```

Change it to (add `mod downloads;` and the re-export):

```rust
mod downloads;
mod reader;
mod tab;
mod tab_manager;
mod webview;

pub use downloads::{
    default_download_dir, is_forbidden_download_destination, sanitize_download_filename,
};
```

(Keep the existing `pub use reader::…`, `pub use tab::…`, `pub use tab_manager::…`, `pub use webview::…` lines unchanged. `FALLBACK_DOWNLOAD_FILENAME` stays module-internal — it is referenced by `sanitize_download_filename` itself and by tests.)

In `src/commands/mod.rs`, replace:

```rust
pub(crate) use palette::{
    display_navigation_url, is_safe_navigation_url, sanitize_ui_text, should_allow_navigation,
};
```

with:

```rust
pub(crate) use palette::{
    display_navigation_url, is_bidi_control, is_safe_navigation_url, sanitize_ui_text,
    should_allow_navigation,
};
```

- [ ] **Step 2: Run tests — expect fail**

Run: `cargo test -- browser::downloads`

Expected: compile error — `sanitize_download_filename`, `default_download_dir`, `destination_is_under`, `is_forbidden_download_destination`, `FALLBACK_DOWNLOAD_FILENAME` not found.

- [ ] **Step 3: Minimal implementation**

Add above the `#[cfg(test)]` module in `src/browser/downloads.rs`:

```rust
pub const FALLBACK_DOWNLOAD_FILENAME: &str = "telechargement";

/// Garde uniquement le dernier composant de chemin (aucun `/` ni `\`),
/// retire les caractères de contrôle et bidi, retire les points de tête
/// (pas de fichier caché), tronque à 128 caractères.
/// Résultat vide → FALLBACK_DOWNLOAD_FILENAME.
pub fn sanitize_download_filename(input: &str) -> String {
    let last_component = input
        .rsplit(|c| c == '/' || c == '\\')
        .next()
        .unwrap_or("");
    let cleaned: String = last_component
        .chars()
        .filter(|ch| !ch.is_control() && !is_bidi_control(*ch))
        .collect();
    let visible = cleaned.trim_start_matches('.');
    let truncated: String = visible.chars().take(128).collect();
    if truncated.is_empty() {
        FALLBACK_DOWNLOAD_FILENAME.to_string()
    } else {
        truncated
    }
}

/// dirs::download_dir() (XDG), sinon ~/Downloads, sinon ".".
pub fn default_download_dir() -> PathBuf {
    dirs::download_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join("Downloads")))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// true si le chemin choisi est sous ~/.config/liteweb, ~/.local/share/liteweb
/// ou ~/.cache/liteweb (profil, base SQLite, données WebKit).
pub fn is_forbidden_download_destination(path: &Path) -> bool {
    let mut roots = Vec::new();
    if let Some(dir) = dirs::config_dir() {
        roots.push(dir.join("liteweb"));
    }
    if let Some(dir) = dirs::data_dir() {
        roots.push(dir.join("liteweb"));
    }
    if let Some(dir) = dirs::cache_dir() {
        roots.push(dir.join("liteweb"));
    }
    destination_is_under(path, &roots)
}

/// Le parent est canonicalisé pour résister aux symlinks ; si la
/// canonicalisation échoue (répertoire pas encore créé), on compare le
/// chemin brut — fail-closed pour la détection.
fn destination_is_under(path: &Path, roots: &[PathBuf]) -> bool {
    let parent = path.parent().unwrap_or(path);
    let canonical_parent = parent
        .canonicalize()
        .unwrap_or_else(|_| parent.to_path_buf());
    roots.iter().any(|root| {
        let canonical_root = root.canonicalize().unwrap_or_else(|_| root.clone());
        canonical_parent.starts_with(&canonical_root)
    })
}
```

- [ ] **Step 4: Run tests — expect pass**

Run: `cargo test -- browser::downloads`

Expected: all 9 new tests PASS.

Note: `cargo build` (without the test harness) may warn that `default_download_dir` / `is_forbidden_download_destination` are never used until Task 4 wires them into `window.rs`. This is an expected transient warning — do NOT "fix" it by removing the exports.

- [ ] **Step 5: Commit**

```bash
git add src/browser/downloads.rs src/browser/mod.rs src/commands/mod.rs
git commit -m "feat: download filename and destination helpers"
```

---

### Task 2: `Storage` — `downloads` table + status lifecycle

**Files:**
- Modify: `src/storage/db.rs` (`init_schema` ~line 82, new struct/consts/methods after `save_suspended_tab`, tests at the bottom)
- Modify: `src/storage/mod.rs`
- Test: `src/storage/db.rs` (`#[cfg(test)]` module)

**Interfaces:**
- Produces (used by Tasks 4 and 5):
  - `pub struct DownloadEntry { pub url: String, pub filename: String, pub path: String, pub status: String, pub started_at: DateTime<Utc> }`
  - `pub const DOWNLOAD_STATUS_IN_PROGRESS: &str = "in_progress";` / `DOWNLOAD_STATUS_COMPLETED: &str = "completed";` / `DOWNLOAD_STATUS_FAILED: &str = "failed";`
  - `pub fn add_download(&self, url: &str, filename: &str, path: &str) -> Option<i64>` — inserts with status `in_progress`, returns the rowid, caps the table at 200 rows, `None` + `eprintln!` on error.
  - `pub fn mark_download(&self, id: i64, status: &str)`
  - `pub fn recent_downloads(&self, limit: usize) -> Vec<DownloadEntry>` — newest first.
- Consumes: nothing new (same `conn()` / `params!` / `eprintln!` pattern as `add_history`).

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `src/storage/db.rs`:

```rust
#[test]
fn download_roundtrip_and_status_transitions() {
    let storage = Storage::open_in_memory().unwrap();
    let id = storage
        .add_download("https://example.com/file.pdf", "file.pdf", "/tmp/file.pdf")
        .unwrap();

    let entries = storage.recent_downloads(5);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].url, "https://example.com/file.pdf");
    assert_eq!(entries[0].filename, "file.pdf");
    assert_eq!(entries[0].path, "/tmp/file.pdf");
    assert_eq!(entries[0].status, DOWNLOAD_STATUS_IN_PROGRESS);

    storage.mark_download(id, DOWNLOAD_STATUS_COMPLETED);
    assert_eq!(
        storage.recent_downloads(5)[0].status,
        DOWNLOAD_STATUS_COMPLETED
    );

    storage.mark_download(id, DOWNLOAD_STATUS_FAILED);
    assert_eq!(storage.recent_downloads(5)[0].status, DOWNLOAD_STATUS_FAILED);
}

#[test]
fn downloads_are_capped_at_200_rows() {
    let storage = Storage::open_in_memory().unwrap();
    for i in 0..205 {
        // `add_download` returns Option<i64> (#[must_use]) — discard explicitly.
        let _ = storage.add_download(
            &format!("https://example.com/file-{i}.bin"),
            &format!("file-{i}.bin"),
            "/tmp/f.bin",
        );
    }
    assert_eq!(storage.recent_downloads(500).len(), 200);
}
```

- [ ] **Step 2: Run tests — expect fail**

Run: `cargo test -- storage`

Expected: compile error — `add_download` / `DOWNLOAD_STATUS_IN_PROGRESS` not found.

- [ ] **Step 3: Minimal implementation**

1. In `src/storage/db.rs`, after the `TabSnapshot` struct (~line 24), add:

```rust
#[derive(Debug, Clone)]
pub struct DownloadEntry {
    pub url: String,
    pub filename: String,
    pub path: String,
    pub status: String,
    pub started_at: DateTime<Utc>,
}

pub const DOWNLOAD_STATUS_IN_PROGRESS: &str = "in_progress";
pub const DOWNLOAD_STATUS_COMPLETED: &str = "completed";
pub const DOWNLOAD_STATUS_FAILED: &str = "failed";
```

2. In `init_schema`, inside the `execute_batch` SQL string, right after the `suspended_tabs` table and before the `idx_history_visited` index, add:

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

3. After `save_suspended_tab` (~line 188), add the three methods:

```rust
pub fn add_download(&self, url: &str, filename: &str, path: &str) -> Option<i64> {
    let now = Utc::now().to_rfc3339();
    let id = {
        let conn = self.conn();
        if let Err(error) = conn.execute(
            "INSERT INTO downloads (url, filename, path, status, started_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![url, filename, path, DOWNLOAD_STATUS_IN_PROGRESS, now],
        ) {
            eprintln!("LiteWeb: impossible d'enregistrer le téléchargement: {error}");
            return None;
        }
        conn.last_insert_rowid()
    };
    if let Err(error) = self.conn().execute(
        "DELETE FROM downloads WHERE id NOT IN (SELECT id FROM downloads ORDER BY started_at DESC, id DESC LIMIT 200)",
        [],
    ) {
        eprintln!("LiteWeb: impossible de limiter les téléchargements: {error}");
    }
    Some(id)
}

pub fn mark_download(&self, id: i64, status: &str) {
    if let Err(error) = self.conn().execute(
        "UPDATE downloads SET status = ?1 WHERE id = ?2",
        params![status, id],
    ) {
        eprintln!("LiteWeb: impossible de mettre à jour le téléchargement: {error}");
    }
}

pub fn recent_downloads(&self, limit: usize) -> Vec<DownloadEntry> {
    let conn = self.conn();
    let Ok(mut stmt) = conn.prepare(
        "SELECT url, filename, path, status, started_at FROM downloads ORDER BY started_at DESC, id DESC LIMIT ?1",
    ) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map(params![limit as i64], |row| {
        let started_at: String = row.get(4)?;
        Ok(DownloadEntry {
            url: row.get(0)?,
            filename: row.get(1)?,
            path: row.get(2)?,
            status: row.get(3)?,
            started_at: started_at.parse().unwrap_or_else(|_| Utc::now()),
        })
    }) else {
        return Vec::new();
    };
    rows.filter_map(Result::ok).collect()
}
```

(The `, id DESC` tiebreaker matters: `started_at` has second precision, so 205 inserts in the cap test share timestamps. The `conn()` mutex is NOT reentrant — note how `add_download` drops the first guard before the DELETE, same as `add_history`.)

4. In `src/storage/mod.rs`, replace:

```rust
pub use db::Storage;
```

with:

```rust
pub use db::{
    DownloadEntry, Storage, DOWNLOAD_STATUS_COMPLETED, DOWNLOAD_STATUS_FAILED,
    DOWNLOAD_STATUS_IN_PROGRESS,
};
```

- [ ] **Step 4: Run tests — expect pass**

Run: `cargo test -- storage`

Expected: the 2 new tests PASS plus regressions `history_roundtrip`, `bookmark_roundtrip`, `file_database_is_private`, `rejects_database_symlink`.

- [ ] **Step 5: Commit**

```bash
git add src/storage/db.rs src/storage/mod.rs
git commit -m "feat: downloads table with in_progress/completed/failed lifecycle"
```

---

### Task 3: `Response` policy arm — unsupported MIME becomes a download *request*

Safe intermediate state: the cancel-all `download-started` handler in `create_web_context` is still active after this task, so every `decision.download()` is immediately cancelled — still default-deny, nothing is written.

**Files:**
- Modify: `src/browser/webview.rs` (`PolicyDecisionType::Response` arm inside `connect_decide_policy`, ~lines 186–200)

**Interfaces:**
- Consumes: existing `should_allow_navigation`, `ResponsePolicyDecisionExt::is_mime_type_supported`, `PolicyDecisionExt::download` (trait already imported via `PolicyDecisionExt`).
- Produces: unsupported-MIME responses (including `Content-Disposition: attachment`, which WebKit reports as unsupported) emit `download-started` on the `WebContext` instead of being silently ignored. Task 4's `wire_downloads` consumes that signal.

- [ ] **Step 1: Rewrite the Response arm**

In `src/browser/webview.rs`, the current arm is:

```rust
PolicyDecisionType::Response => {
    let Some(response) = decision.dynamic_cast_ref::<ResponsePolicyDecision>() else {
        decision.ignore();
        return true;
    };
    let uri = response.request().and_then(|request| request.uri());
    if !should_allow_navigation(uri.as_deref(), |candidate| {
        blocker_policy.check(candidate)
    }) || !response.is_mime_type_supported()
    {
        decision.ignore();
        return true;
    }
    false
}
```

Replace it with (the `should_allow_navigation` check MUST stay first — a blocked/unsafe URI is `ignore()`d, never turned into a download):

```rust
PolicyDecisionType::Response => {
    let Some(response) = decision.dynamic_cast_ref::<ResponsePolicyDecision>() else {
        decision.ignore();
        return true;
    };
    let uri = response.request().and_then(|request| request.uri());
    if !should_allow_navigation(uri.as_deref(), |candidate| {
        blocker_policy.check(candidate)
    }) {
        decision.ignore();
        return true;
    }
    if !response.is_mime_type_supported() {
        // La réponse ne navigue jamais et n'écrit jamais directement :
        // elle devient une *demande* de téléchargement (download-started),
        // qui reste default-deny en aval (wire_downloads).
        decision.download();
        return true;
    }
    false
}
```

- [ ] **Step 2: Build + regression tests**

Run: `cargo build`
Expected: success.

Run: `cargo test`
Expected: all tests PASS (no unit test can exercise WebKit policy objects; behavior is covered by Task 5's manual checks).

- [ ] **Step 3: Commit**

```bash
git add src/browser/webview.rs
git commit -m "feat: unsupported MIME responses request a download instead of ignore"
```

---

### Task 4: `wire_downloads` — save dialog pipeline, replaces the cancel-all handler

**CRITICAL:** the removal of the cancel-all handler in `webview.rs` and the addition of `wire_downloads` in `window.rs` land in this single commit. Never commit one without the other.

**Files:**
- Modify: `src/browser/webview.rs` (`create_web_context`, ~lines 211–227; imports ~line 6)
- Modify: `src/ui/window.rs` (imports ~lines 14, 29–37; `new()` ~line 178; new methods after `close_current_tab` ~line 1005)

**Interfaces:**
- Consumes: `sanitize_download_filename`, `default_download_dir`, `is_forbidden_download_destination` (Task 1); `Storage::add_download` / `mark_download`, `DOWNLOAD_STATUS_COMPLETED` / `DOWNLOAD_STATUS_FAILED` (Task 2); `should_allow_navigation` (`crate::commands`); existing `Self::show_message`.
- Produces:
  - `fn wire_downloads(state: Rc<RefCell<AppState>>)` — connects `download-started` on the shared `WebContext`.
  - `fn decide_download_destination(state: Rc<RefCell<AppState>>, download: &Download, url: &str, suggested: &str) -> bool` — returns `true` in ALL paths.
  - `fn prompt_download_destination(filename: &str) -> Option<PathBuf>`.

- [ ] **Step 1: Remove the cancel-all handler from `create_web_context`**

In `src/browser/webview.rs`, delete these lines from `create_web_context`:

```rust
context.connect_download_started(|_, download| {
    download.cancel();
});
```

and put this doc comment on the function so the invariant survives:

```rust
/// NOTE téléchargements : aucun handler `download-started` n'est connecté ici.
/// `BrowserWindow::wire_downloads` DOIT être connecté avant le premier
/// chargement (avant `render_tabs`), sinon le handler par défaut de WebKit
/// écrirait dans le dossier XDG sans confirmation.
#[allow(deprecated)]
pub fn create_web_context() -> WebContext {
```

`DownloadExt` is now unused in `webview.rs`: remove it from the `use webkit2gtk::{…}` list at the top of the file.

- [ ] **Step 2: Add imports and the `wire_downloads` call in `window.rs`**

1. Top of `src/ui/window.rs`, extend the imports:

```rust
use std::path::PathBuf;
```

Replace the webkit2gtk import line:

```rust
use webkit2gtk::{LoadEvent, UserContentManager, WebResourceExt, WebView, WebViewExt};
```

with:

```rust
use webkit2gtk::{
    Download, DownloadExt, LoadEvent, URIRequestExt, UserContentManager, WebContextExt,
    WebResourceExt, WebView, WebViewExt,
};
```

Replace the `crate::browser` import block so it also pulls the Task 1 helpers:

```rust
use crate::browser::{
    apply_webview_policy, create_user_content_manager, create_web_context, create_webview,
    default_download_dir, flatten_html, is_forbidden_download_destination,
    sandbox_runtime_available, sanitize_download_filename, set_archaic_stylesheet, NewTabHandler,
    TabManager,
};
```

Replace the `crate::commands` import:

```rust
use crate::commands::{
    display_navigation_url, is_safe_navigation_url, sanitize_ui_text, should_allow_navigation,
    CommandAction, CommandPalette,
};
```

Replace the `crate::storage` import:

```rust
use crate::storage::{Storage, DOWNLOAD_STATUS_COMPLETED, DOWNLOAD_STATUS_FAILED};
```

2. In `BrowserWindow::new`, the wiring block currently reads:

```rust
Self::wire_toolbar(&toolbar, state.clone());
Self::wire_command_bar(&command_bar, state.clone());
Self::wire_shortcuts(&window, state.clone());
Self::wire_notebook(&notebook, state.clone());
Self::render_tabs(state.clone(), true);
```

Insert `wire_downloads` BEFORE `render_tabs` (before any page load):

```rust
Self::wire_toolbar(&toolbar, state.clone());
Self::wire_command_bar(&command_bar, state.clone());
Self::wire_shortcuts(&window, state.clone());
Self::wire_notebook(&notebook, state.clone());
Self::wire_downloads(state.clone());
Self::render_tabs(state.clone(), true);
```

- [ ] **Step 3: Implement the three methods**

Add after `close_current_tab` in `src/ui/window.rs`:

```rust
fn wire_downloads(state: Rc<RefCell<AppState>>) {
    let (web_context, blocker) = {
        let st = state.borrow();
        (st.web_context.clone(), st.blocker.clone())
    };
    web_context.connect_download_started(move |_, download| {
        // Re-vérifie l'URI de la requête : couvre aussi WebView::download_uri
        // et tout chemin futur vers download-started.
        let Some(uri) = download.request().and_then(|request| request.uri()) else {
            download.cancel(); // URI absente → default deny, silencieux
            return;
        };
        if !should_allow_navigation(Some(uri.as_str()), |candidate| blocker.check(candidate)) {
            // Schéma non http/https, credentials dans l'URL, host bloqué.
            download.cancel();
            return;
        }
        let url = uri.to_string();
        let state_dest = state.clone();
        download.connect_decide_destination(move |download, suggested| {
            Self::decide_download_destination(state_dest.clone(), download, &url, suggested)
        });
    });
}

/// Retourne `true` dans TOUS les cas (destination posée ou `cancel()`) :
/// le handler par défaut de WebKit ne s'exécute jamais.
fn decide_download_destination(
    state: Rc<RefCell<AppState>>,
    download: &Download,
    url: &str,
    suggested: &str,
) -> bool {
    let filename = sanitize_download_filename(suggested);
    // Clone storage puis relâche l'emprunt RefCell AVANT dialog.run()
    // (boucle principale imbriquée — un emprunt tenu paniquerait).
    let storage = state.borrow().storage.clone();

    let Some(path) = Self::prompt_download_destination(&filename) else {
        download.cancel(); // rejet / Échap : default deny, silencieux
        return true;
    };

    if is_forbidden_download_destination(&path) {
        download.cancel();
        Self::show_message(
            "Téléchargement refusé",
            "Impossible d'enregistrer un fichier dans le profil LiteWeb.",
        );
        return true;
    }

    let Ok(destination_uri) = glib::filename_to_uri(&path, None) else {
        download.cancel(); // chemin non convertible en URI
        return true;
    };

    // Le dialogue a déjà confirmé l'écrasement.
    download.set_allow_overwrite(true);
    download.set_destination(&destination_uri);

    // Best effort (comme add_history) : si la base est indisponible,
    // le téléchargement continue sans suivi.
    if let Some(id) = storage.add_download(url, &filename, &path.to_string_lossy()) {
        let storage_done = storage.clone();
        download.connect_finished(move |_| {
            storage_done.mark_download(id, DOWNLOAD_STATUS_COMPLETED);
        });
        download.connect_failed(move |_, _| {
            storage.mark_download(id, DOWNLOAD_STATUS_FAILED);
        });
    }
    true
}

fn prompt_download_destination(filename: &str) -> Option<PathBuf> {
    let dialog = gtk::FileChooserNative::new(
        Some("Enregistrer le fichier"),
        None::<&gtk::Window>,
        gtk::FileChooserAction::Save,
        Some("Enregistrer"),
        Some("Annuler"),
    );
    dialog.set_do_overwrite_confirmation(true);
    dialog.set_current_folder(default_download_dir());
    dialog.set_current_name(filename);
    let response = dialog.run();
    let path = dialog.filename();
    if response == gtk::ResponseType::Accept {
        path
    } else {
        None
    }
}
```

Notes for the implementer:
- No interaction with `tabs` anywhere in this pipeline: downloads run in WebKit's network process, wake **no** suspended tab, and keep working in Ultra/éco mode.
- Simultaneous downloads produce sequential nested dialogs (GTK nested main loops) — no UI queue, per spec.
- A SQLite row is only created after the dialog was accepted AND the destination passed the profile check: a rejected dialog leaves no trace.

- [ ] **Step 4: Build + regression tests**

Run: `cargo build`
Expected: success, and the transient "never used" warnings from Task 1 are gone.

Run: `cargo test`
Expected: all tests PASS (GTK dialogs and WebKit `Download` objects have no headless harness; Task 5 carries the manual checklist).

- [ ] **Step 5: Commit (single atomic commit — both files)**

```bash
git add src/browser/webview.rs src/ui/window.rs
git commit -m "feat: explicit save dialog for downloads, default deny preserved"
```

---

### Task 5: `:download` list UI + parser regression + manual verification

**Files:**
- Modify: `src/ui/window.rs` (`KEYBAR_COMMAND` ~line 20, `run_command` `DownloadList` arm ~line 1174, new `show_downloads` near `show_history`)
- Modify: `src/commands/palette.rs` (tests only — parsing `"download" | "dl" => CommandAction::DownloadList` already exists at line 76)
- Test: `src/commands/palette.rs` (`#[cfg(test)]` module)

**Interfaces:**
- Consumes: `Storage::recent_downloads(20)` and the three `DOWNLOAD_STATUS_*` consts (Task 2, already imported in Task 4 — add `DOWNLOAD_STATUS_IN_PROGRESS` if the match below needs it spelled out; the version below uses a catch-all for « en cours » so no extra import is required); existing `sanitize_ui_text`, `display_navigation_url`, `Self::show_message`.
- Produces: `fn show_downloads(state: Rc<RefCell<AppState>>)`.

- [ ] **Step 1: Write the parser regression tests**

Add to the `tests` module in `src/commands/palette.rs` (these pass immediately — regression guards required by the spec):

```rust
#[test]
fn parses_download_list() {
    assert_eq!(
        CommandPalette::parse(":download"),
        CommandAction::DownloadList
    );
}

#[test]
fn parses_dl_alias() {
    assert_eq!(CommandPalette::parse(":dl"), CommandAction::DownloadList);
}
```

Run: `cargo test -- commands::palette`
Expected: both PASS (parsing already exists — these lock it in).

- [ ] **Step 2: Implement `show_downloads` and wire it**

1. Add after `show_history` in `src/ui/window.rs`:

```rust
fn show_downloads(state: Rc<RefCell<AppState>>) {
    let entries = state.borrow().storage.recent_downloads(20);
    let text = if entries.is_empty() {
        "Aucun téléchargement.".to_string()
    } else {
        entries
            .iter()
            .map(|entry| {
                let status = match entry.status.as_str() {
                    DOWNLOAD_STATUS_COMPLETED => "terminé",
                    DOWNLOAD_STATUS_FAILED => "échoué",
                    _ => "en cours",
                };
                format!(
                    "• {} — {} → {} [{}]",
                    sanitize_ui_text(&entry.filename),
                    display_navigation_url(&entry.url),
                    sanitize_ui_text(&entry.path),
                    status
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Self::show_message("Téléchargements", &text);
}
```

2. In `run_command`, replace:

```rust
CommandAction::DownloadList => Self::show_message(
    "Téléchargements",
    "Gestion des téléchargements — bientôt disponible.",
),
```

with:

```rust
CommandAction::DownloadList => Self::show_downloads(state_cmd.clone()),
```

3. Update `KEYBAR_COMMAND` (top of `src/ui/window.rs`) to mention `:download`:

```rust
const KEYBAR_COMMAND: &str =
    ":open :tab new|next|prev|N :suspend :suspend-all :eco on|off|aggressive|ultra :bookmark :history :download  |  Entrée → exécuter  |  Échap → annuler";
```

- [ ] **Step 3: Build + full test suite**

Run: `cargo build`
Expected: success.

Run: `cargo test`
Expected: ALL tests PASS — including `browser::downloads` (Task 1), `storage` (Task 2), and `commands::palette` regressions.

- [ ] **Step 4: Manual verification (GTK, no headless harness — same limit as the audit)**

Run: `cargo run` and check:

1. Navigate to a page serving a real attachment (e.g. a `.tar.gz` or a `Content-Disposition: attachment` link): the « Enregistrer le fichier » dialog opens, suggested folder is `~/Téléchargements` (XDG), suggested name is sanitized.
2. « Annuler » / Échap → no file anywhere on disk, `:download` shows no new entry.
3. « Enregistrer » → file written at the chosen path; `:download` shows the entry as « en cours » then « terminé » once finished.
4. Choose a destination under `~/.config/liteweb` → refused with « Téléchargement refusé », nothing written.
5. Suspend tabs (`:suspend-all`) then download from the active tab: 💤 tabs stay asleep during the download.
6. `:eco ultra` → downloads still work (not page JS).
7. A supported-MIME page (normal HTML) still navigates normally.
8. `:download` and `:dl` both show the « Téléchargements » list dialog; format is `• nom — URL → chemin [statut]`.
9. No file is ever opened automatically after download.

- [ ] **Step 5: Commit**

```bash
git add src/ui/window.rs src/commands/palette.rs
git commit -m "feat: :download lists the 20 most recent downloads"
```

---

## Spec coverage check

| Spec item | Task |
|-----------|------|
| `sanitize_download_filename` (traversal, backslash, hidden files, control+bidi, 128-char cap, fallback) | 1 |
| `default_download_dir` (XDG → ~/Downloads → ".") | 1 |
| `is_forbidden_download_destination` over the three profile roots, canonicalized parent | 1 |
| `destination_is_under(path, roots)` testable with injected roots | 1 |
| `is_bidi_control` re-exported by `src/commands/mod.rs` | 1 |
| `downloads` table + index, `DownloadEntry`, status consts | 2 |
| `add_download` → rowid, cap 200; `mark_download`; `recent_downloads` | 2 |
| Storage tests: roundtrip `in_progress` → `completed`, cap 200 | 2 |
| Response arm: `should_allow_navigation` first, then unsupported MIME → `decision.download()`, supported → unchanged | 3 |
| Cancel-all handler removed + doc comment on `create_web_context` | 4 |
| Same-commit constraint (handler swap) | 4 (single commit) |
| `wire_downloads` connected in `new()` before `render_tabs` | 4 |
| `download-started` re-checks `should_allow_navigation`; missing URI / bad scheme / credentials / blocked host → `cancel()` | 4 |
| `decide_download_destination` returns `true` in all paths | 4 |
| `RefCell` borrow released before `dialog.run()` | 4 |
| `FileChooserNative` Save, « Enregistrer » / « Annuler », overwrite confirmation, XDG folder, sanitized name | 4 |
| Forbidden profile destination → `cancel()` + « Téléchargement refusé » | 4 |
| `filename_to_uri` failure → `cancel()` | 4 |
| `set_allow_overwrite(true)` + `set_destination` + SQLite row only after acceptance | 4 |
| `connect_finished` → completed, `connect_failed` → failed (best effort) | 4 |
| No tab interaction, suspended tabs stay asleep, Ultra still downloads | 4 (code) + 5 (manual) |
| HTTP Basic stays cancelled (download behind Basic → « échoué ») | (unchanged `connect_authenticate`) + Global Constraints |
| `connect_run_file_chooser` stays `cancel()` (uploads out of scope) | (unchanged) + Global Constraints |
| `show_downloads`: `recent_downloads(20)`, `• nom — URL → chemin [statut]`, sanitized text | 5 |
| `KEYBAR_COMMAND` mentions `:download` | 5 |
| Palette regression `:download` / `:dl` → `DownloadList` | 5 |
| Manual test checklist (attachment, cancel, accept, forbidden dest, 💤, Ultra, supported MIME) | 5 |
| No new shortcut, no toolbar button, never auto-open, no README/roadmap edit | Global Constraints (nothing implemented) |
