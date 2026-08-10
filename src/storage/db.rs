use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub visited_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct Bookmark {
    pub url: String,
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct TabSnapshot {
    pub url: String,
    pub title: String,
    pub scroll_x: i32,
    pub scroll_y: i32,
}

#[derive(Clone)]
pub struct Storage {
    conn: std::sync::Arc<std::sync::Mutex<Connection>>,
}

impl Storage {
    pub fn open() -> Self {
        let path = Self::db_path();
        match Self::open_file(&path) {
            Ok(storage) => storage,
            Err(error) => {
                eprintln!(
                    "LiteWeb: stockage persistant indisponible ({error}); utilisation d'une base temporaire"
                );
                Self::open_in_memory().expect("failed to initialize fallback sqlite database")
            }
        }
    }

    fn open_file(path: &Path) -> rusqlite::Result<Self> {
        prepare_private_database_path(path).map_err(|error| {
            rusqlite::Error::InvalidPath(PathBuf::from(format!("{}: {error}", path.display())))
        })?;

        let conn = Connection::open(path)?;
        set_private_file_permissions(path).map_err(|error| {
            rusqlite::Error::InvalidPath(PathBuf::from(format!("{}: {error}", path.display())))
        })?;
        let storage = Self {
            conn: std::sync::Arc::new(std::sync::Mutex::new(conn)),
        };
        storage.init_schema()?;
        Ok(storage)
    }

    fn open_in_memory() -> rusqlite::Result<Self> {
        let storage = Self {
            conn: std::sync::Arc::new(std::sync::Mutex::new(Connection::open_in_memory()?)),
        };
        storage.init_schema()?;
        Ok(storage)
    }

    fn db_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("liteweb")
            .join("liteweb.db")
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn init_schema(&self) -> rusqlite::Result<()> {
        self.conn().execute_batch(
            "
                CREATE TABLE IF NOT EXISTS history (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    url TEXT NOT NULL,
                    title TEXT NOT NULL DEFAULT '',
                    visited_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS bookmarks (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    url TEXT NOT NULL UNIQUE,
                    title TEXT NOT NULL DEFAULT '',
                    created_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS suspended_tabs (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    url TEXT NOT NULL,
                    title TEXT NOT NULL DEFAULT '',
                    scroll_x INTEGER NOT NULL DEFAULT 0,
                    scroll_y INTEGER NOT NULL DEFAULT 0
                );
                CREATE INDEX IF NOT EXISTS idx_history_visited ON history(visited_at DESC);
                ",
        )?;
        Ok(())
    }

    pub fn add_history(&self, url: &str, title: &str) {
        let now = Utc::now().to_rfc3339();
        if let Err(error) = self.conn().execute(
            "INSERT INTO history (url, title, visited_at) VALUES (?1, ?2, ?3)",
            params![url, title, now],
        ) {
            eprintln!("LiteWeb: impossible d'enregistrer l'historique: {error}");
            return;
        }
        if let Err(error) = self.conn().execute(
            "DELETE FROM history WHERE id NOT IN (SELECT id FROM history ORDER BY visited_at DESC LIMIT 500)",
            [],
        ) {
            eprintln!("LiteWeb: impossible de limiter l'historique: {error}");
        }
    }

    pub fn recent_history(&self, limit: usize) -> Vec<HistoryEntry> {
        let conn = self.conn();
        let Ok(mut stmt) = conn.prepare(
            "SELECT url, title, visited_at FROM history ORDER BY visited_at DESC LIMIT ?1",
        ) else {
            return Vec::new();
        };
        let Ok(rows) = stmt.query_map(params![limit as i64], |row| {
            let visited_at: String = row.get(2)?;
            Ok(HistoryEntry {
                url: row.get(0)?,
                title: row.get(1)?,
                visited_at: visited_at.parse().unwrap_or_else(|_| Utc::now()),
            })
        }) else {
            return Vec::new();
        };
        rows.filter_map(Result::ok).collect()
    }

    pub fn add_bookmark(&self, url: &str, title: &str) {
        let now = Utc::now().to_rfc3339();
        if let Err(error) = self.conn().execute(
            "INSERT OR REPLACE INTO bookmarks (url, title, created_at) VALUES (?1, ?2, ?3)",
            params![url, title, now],
        ) {
            eprintln!("LiteWeb: impossible d'enregistrer le favori: {error}");
        }
    }

    pub fn list_bookmarks(&self) -> Vec<Bookmark> {
        let conn = self.conn();
        let Ok(mut stmt) =
            conn.prepare("SELECT url, title FROM bookmarks ORDER BY title COLLATE NOCASE")
        else {
            return Vec::new();
        };
        let Ok(rows) = stmt.query_map([], |row| {
            Ok(Bookmark {
                url: row.get(0)?,
                title: row.get(1)?,
            })
        }) else {
            return Vec::new();
        };
        rows.filter_map(Result::ok).collect()
    }

    pub fn save_suspended_tab(&self, snapshot: &TabSnapshot) {
        if let Err(error) = self.conn().execute(
            "INSERT INTO suspended_tabs (url, title, scroll_x, scroll_y) VALUES (?1, ?2, ?3, ?4)",
            params![
                snapshot.url,
                snapshot.title,
                snapshot.scroll_x,
                snapshot.scroll_y
            ],
        ) {
            eprintln!("LiteWeb: impossible d'enregistrer l'onglet suspendu: {error}");
        }
    }
}

fn prepare_private_database_path(path: &Path) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "chemin sans répertoire parent",
        )
    })?;
    std::fs::create_dir_all(parent)?;
    set_private_dir_permissions(parent)?;

    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "la base doit être un fichier régulier et non un lien symbolique",
                ));
            }
            set_private_file_permissions(path)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            create_private_file(path)?;
        }
        Err(error) => return Err(error),
    }
    Ok(())
}

#[cfg(unix)]
fn create_private_file(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map(drop)
}

#[cfg(not(unix))]
fn create_private_file(path: &Path) -> std::io::Result<()> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map(drop)
}

#[cfg(unix)]
fn set_private_dir_permissions(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn set_private_dir_permissions(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_private_file_permissions(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_roundtrip() {
        let storage = Storage::open_in_memory().unwrap();
        storage.add_history("https://example.com", "Example");
        let entries = storage.recent_history(5);
        assert!(!entries.is_empty());
        assert_eq!(entries[0].url, "https://example.com");
    }

    #[test]
    fn bookmark_roundtrip() {
        let storage = Storage::open_in_memory().unwrap();
        storage.add_bookmark("https://example.com", "Example");
        let bookmarks = storage.list_bookmarks();
        assert!(bookmarks.iter().any(|b| b.url == "https://example.com"));
    }

    #[cfg(unix)]
    #[test]
    fn file_database_is_private() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "liteweb-storage-test-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let path = root.join("profile").join("liteweb.db");
        let storage = Storage::open_file(&path).unwrap();
        drop(storage);

        assert_eq!(
            std::fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_database_symlink() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "liteweb-symlink-test-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let target = root.join("target.db");
        std::fs::write(&target, []).unwrap();
        let link = root.join("liteweb.db");
        symlink(&target, &link).unwrap();

        assert!(Storage::open_file(&link).is_err());

        std::fs::remove_dir_all(root).unwrap();
    }
}
