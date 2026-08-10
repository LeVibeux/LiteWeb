use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::path::PathBuf;

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
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(&path).expect("failed to open sqlite database");
        let storage = Self {
            conn: std::sync::Arc::new(std::sync::Mutex::new(conn)),
        };
        storage.init_schema();
        storage
    }

    fn db_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("liteweb")
            .join("liteweb.db")
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().expect("database lock poisoned")
    }

    fn init_schema(&self) {
        self.conn()
            .execute_batch(
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
            )
            .expect("failed to initialize schema");
    }

    pub fn add_history(&self, url: &str, title: &str) {
        let now = Utc::now().to_rfc3339();
        let _ = self.conn().execute(
            "INSERT INTO history (url, title, visited_at) VALUES (?1, ?2, ?3)",
            params![url, title, now],
        );
        let _ = self.conn().execute(
            "DELETE FROM history WHERE id NOT IN (SELECT id FROM history ORDER BY visited_at DESC LIMIT 500)",
            [],
        );
    }

    pub fn recent_history(&self, limit: usize) -> Vec<HistoryEntry> {
        let conn = self.conn.lock().expect("database lock poisoned");
        let mut stmt = conn
            .prepare(
                "SELECT url, title, visited_at FROM history ORDER BY visited_at DESC LIMIT ?1",
            )
            .expect("history query");
        let rows = stmt
            .query_map(params![limit as i64], |row| {
                let visited_at: String = row.get(2)?;
                Ok(HistoryEntry {
                    url: row.get(0)?,
                    title: row.get(1)?,
                    visited_at: visited_at.parse().unwrap_or_else(|_| Utc::now()),
                })
            })
            .expect("history rows");
        rows.filter_map(Result::ok).collect()
    }

    pub fn add_bookmark(&self, url: &str, title: &str) {
        let now = Utc::now().to_rfc3339();
        let _ = self.conn().execute(
            "INSERT OR REPLACE INTO bookmarks (url, title, created_at) VALUES (?1, ?2, ?3)",
            params![url, title, now],
        );
    }

    pub fn list_bookmarks(&self) -> Vec<Bookmark> {
        let conn = self.conn.lock().expect("database lock poisoned");
        let mut stmt = conn
            .prepare("SELECT url, title FROM bookmarks ORDER BY title COLLATE NOCASE")
            .expect("bookmark query");
        let rows = stmt
            .query_map([], |row| {
                Ok(Bookmark {
                    url: row.get(0)?,
                    title: row.get(1)?,
                })
            })
            .expect("bookmark rows");
        rows.filter_map(Result::ok).collect()
    }

    pub fn save_suspended_tab(&self, snapshot: &TabSnapshot) {
        let _ = self.conn().execute(
            "INSERT INTO suspended_tabs (url, title, scroll_x, scroll_y) VALUES (?1, ?2, ?3, ?4)",
            params![
                snapshot.url,
                snapshot.title,
                snapshot.scroll_x,
                snapshot.scroll_y
            ],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_roundtrip() {
        let storage = Storage::open();
        storage.add_history("https://example.com", "Example");
        let entries = storage.recent_history(5);
        assert!(!entries.is_empty());
        assert_eq!(entries[0].url, "https://example.com");
    }

    #[test]
    fn bookmark_roundtrip() {
        let storage = Storage::open();
        storage.add_bookmark("https://example.com", "Example");
        let bookmarks = storage.list_bookmarks();
        assert!(bookmarks.iter().any(|b| b.url == "https://example.com"));
    }
}
