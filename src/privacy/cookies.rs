use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use webkit2gtk::{
    CookieManagerExt, WebContext, WebContextExt, WebsiteDataManagerExtManual, WebsiteDataTypes,
};

use crate::privacy::PRIVACY_DATA_DIR;

/// Inactivité maximale avant destruction de la sandbox d'un site.
pub const SANDBOX_IDLE_LIMIT: Duration = Duration::minutes(5);
/// Période du timer de purge.
pub const PURGE_TICK: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteSandbox {
    pub origin: String,
    pub dir: PathBuf,
    pub last_activity: DateTime<Utc>,
}

/// Registre des sites visités : « 1 site → 1 sandbox ».
#[derive(Debug, Default)]
pub struct SiteSandboxRegistry {
    root: PathBuf,
    sites: HashMap<String, SiteSandbox>,
}

impl SiteSandboxRegistry {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            sites: HashMap::new(),
        }
    }

    pub fn default_root() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(PRIVACY_DATA_DIR)
    }

    pub fn touch(&mut self, url: &str) -> Option<SiteSandbox> {
        self.touch_at(url, Utc::now())
    }

    pub fn touch_at(&mut self, url: &str, now: DateTime<Utc>) -> Option<SiteSandbox> {
        let origin = site_origin(url)?;
        let dir = self.root.join(sandbox_dir_name(&origin)?);
        let sandbox = self
            .sites
            .entry(origin.clone())
            .or_insert_with(|| SiteSandbox {
                origin: origin.clone(),
                dir: dir.clone(),
                last_activity: now,
            });
        sandbox.last_activity = now;
        if let Err(error) = std::fs::create_dir_all(&sandbox.dir) {
            eprintln!(
                "LiteWeb: sandbox de site indisponible ({}): {error}",
                sandbox.dir.display()
            );
        }
        Some(sandbox.clone())
    }

    pub fn active_origins(&self) -> HashSet<String> {
        self.sites.keys().cloned().collect()
    }

    pub fn purgeable(&self, now: DateTime<Utc>, idle_limit: Duration) -> Vec<SiteSandbox> {
        let mut expired: Vec<_> = self
            .sites
            .values()
            .filter(|sandbox| now - sandbox.last_activity >= idle_limit)
            .cloned()
            .collect();
        expired.sort_by(|a, b| a.origin.cmp(&b.origin));
        expired
    }

    pub fn forget(&mut self, origin: &str) {
        if let Some(sandbox) = self.sites.remove(origin) {
            remove_dir_all_logged(&sandbox.dir);
        }
    }

    pub fn len(&self) -> usize {
        self.sites.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sites.is_empty()
    }
}

/// Origine `scheme://host[:port]`, hôte en minuscules, port par défaut omis.
pub fn site_origin(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let host = parsed.host_str()?.to_ascii_lowercase();
    if host.is_empty() || host.bytes().all(|b| b == b'.') {
        return None;
    }
    match parsed.port() {
        Some(port) => Some(format!("{}://{}:{}", parsed.scheme(), host, port)),
        None => Some(format!("{}://{}", parsed.scheme(), host)),
    }
}

/// Nom de répertoire sûr : aucun séparateur, aucun `..`.
pub fn sandbox_dir_name(origin: &str) -> Option<String> {
    let parsed = url::Url::parse(origin).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    if host.is_empty() || host.contains('/') {
        return None;
    }
    let name = match parsed.port() {
        Some(port) => format!("{}__{}_{}", parsed.scheme(), host, port),
        None => format!("{}__{}", parsed.scheme(), host),
    };
    let clean: String = name
        .chars()
        .map(|ch| match ch {
            'a'..='z' | '0'..='9' | '-' | '.' | '_' => ch,
            _ => '_',
        })
        .collect();
    if clean.contains("..") || clean.is_empty() {
        None
    } else {
        Some(clean)
    }
}

pub fn sandbox_dirs() -> Vec<PathBuf> {
    sandbox_dirs_for_root(&SiteSandboxRegistry::default_root())
}

pub fn sandbox_dirs_for_root(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    dirs
}

/// Règle « 1 cookie → 1 site » + refus des traqueurs même first-party.
pub fn should_accept_cookie(first_party_host: &str, cookie_domain: &str, is_tracker: bool) -> bool {
    if is_tracker {
        return false;
    }
    let first_party = first_party_host
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    let domain = cookie_domain
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    if first_party.is_empty() || domain.is_empty() {
        return false;
    }
    first_party == domain || first_party.ends_with(&format!(".{domain}"))
}

/// Un cookie dont le domaine n'est pas un site visité est purgé.
pub fn should_delete_cookie(cookie_domain: &str, allowed_origins: &HashSet<String>) -> bool {
    let domain = cookie_domain
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    if domain.is_empty() {
        return true;
    }
    !allowed_origins
        .iter()
        .filter_map(|origin| url::Url::parse(origin).ok())
        .any(|parsed| {
            parsed
                .host_str()
                .is_some_and(|host| should_accept_cookie(host, &domain, false))
        })
}

/// Destruction complète des sandbox périmées. Renvoie le nombre de sites purgés.
pub fn purge_expired_sandboxes(context: &WebContext, expired: &[SiteSandbox]) -> usize {
    if expired.is_empty() {
        return 0;
    }
    let idle_timespan = SANDBOX_IDLE_LIMIT
        .num_microseconds()
        .map(glib::TimeSpan::from_microseconds)
        .unwrap_or(glib::TimeSpan(0));
    if let Some(manager) = context.website_data_manager() {
        let (tx, rx) = std::sync::mpsc::channel();
        manager.clear(
            WebsiteDataTypes::ALL,
            idle_timespan,
            None::<&gio::Cancellable>,
            move |result| {
                let _ = tx.send(result);
            },
        );
        if let Ok(result) = rx.recv_timeout(std::time::Duration::from_secs(5)) {
            if let Err(error) = result {
                eprintln!("LiteWeb: purge du stockage WebKit impossible: {error}");
            }
        }
    }
    if let Some(cookies) = context.cookie_manager() {
        for sandbox in expired {
            if let Ok(parsed) = url::Url::parse(&sandbox.origin) {
                if let Some(host) = parsed.host_str() {
                    #[allow(deprecated)]
                    cookies.delete_cookies_for_domain(host);
                }
            }
        }
    }
    for sandbox in expired {
        remove_dir_all_logged(&sandbox.dir);
    }
    expired.len()
}

fn remove_dir_all_logged(path: &Path) {
    if !path.exists() {
        return;
    }
    if let Err(error) = std::fs::remove_dir_all(path) {
        eprintln!(
            "LiteWeb: suppression de la sandbox {} impossible: {error}",
            path.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use std::collections::HashSet;

    fn unique_root(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "liteweb-privacy-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ))
    }

    #[test]
    fn site_origin_is_scheme_host_port_only() {
        assert_eq!(
            site_origin("https://Example.COM:443/a/b?c=d#e").as_deref(),
            Some("https://example.com")
        );
        assert_eq!(
            site_origin("http://localhost:8080/x").as_deref(),
            Some("http://localhost:8080")
        );
        assert_eq!(site_origin("about:blank"), None);
        assert_eq!(site_origin("file:///etc/passwd"), None);
        assert_eq!(site_origin("not a url"), None);
    }

    #[test]
    fn sandbox_dir_name_is_filesystem_safe() {
        assert_eq!(
            sandbox_dir_name("https://example.com").as_deref(),
            Some("https__example.com")
        );
        assert_eq!(
            sandbox_dir_name("http://localhost:8080").as_deref(),
            Some("http__localhost_8080")
        );
        assert_eq!(sandbox_dir_name("../../etc"), None);
    }

    #[test]
    fn registry_touch_creates_one_sandbox_per_site() {
        let root = unique_root("touch");
        let mut registry = SiteSandboxRegistry::new(root.clone());
        let first = registry
            .touch("https://example.com/page")
            .expect("sandbox créée");
        assert_eq!(first.origin, "https://example.com");
        assert!(first.dir.starts_with(&root));
        registry.touch("https://example.com/other").unwrap();
        registry.touch("https://other.example/page").unwrap();
        registry.touch("about:blank");
        assert_eq!(registry.len(), 2);
        let origins = registry.active_origins();
        assert!(origins.contains("https://example.com"));
        assert!(origins.contains("https://other.example"));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn purgeable_is_empty_before_the_five_minute_idle_limit() {
        let root = unique_root("idle");
        let mut registry = SiteSandboxRegistry::new(root.clone());
        registry.touch("https://example.com").unwrap();
        let now = Utc::now();
        assert!(registry.purgeable(now, Duration::minutes(5)).is_empty());
        assert_eq!(
            registry
                .purgeable(
                    now + Duration::minutes(4) + Duration::seconds(59),
                    Duration::minutes(5)
                )
                .len(),
            0
        );
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn purgeable_lists_sites_idle_for_five_minutes() {
        let root = unique_root("expired");
        let mut registry = SiteSandboxRegistry::new(root.clone());
        registry.touch("https://old.example").unwrap();
        registry.touch("https://fresh.example").unwrap();
        let now = Utc::now() + Duration::minutes(6);
        registry.touch_at("https://fresh.example", now);
        let expired = registry.purgeable(now, Duration::minutes(5));
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].origin, "https://old.example");
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn forget_removes_the_sandbox_from_disk_and_registry() {
        let root = unique_root("forget");
        let mut registry = SiteSandboxRegistry::new(root.clone());
        let sandbox = registry.touch("https://example.com").unwrap();
        std::fs::create_dir_all(sandbox.dir.join("indexeddb")).unwrap();
        registry.forget("https://example.com");
        assert!(
            !sandbox.dir.exists(),
            "le répertoire du site doit disparaître"
        );
        assert_eq!(registry.len(), 0);
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn third_party_and_tracker_cookies_are_refused() {
        assert!(should_accept_cookie("example.com", "example.com", false));
        assert!(should_accept_cookie(
            "www.example.com",
            "example.com",
            false
        ));
        assert!(should_accept_cookie("EXAMPLE.com", ".example.com", false));
        assert!(!should_accept_cookie(
            "example.com",
            "ads.example.net",
            false
        ));
        assert!(!should_accept_cookie("example.com", "example.com", true));
        assert!(!should_accept_cookie("", "example.com", false));
    }

    #[test]
    fn cookies_outside_the_visited_sites_are_deleted() {
        let mut allowed = HashSet::new();
        allowed.insert("https://example.com".to_string());
        assert!(!should_delete_cookie("example.com", &allowed));
        assert!(!should_delete_cookie(".example.com", &allowed));
        assert!(should_delete_cookie("doubleclick.net", &allowed));
        assert!(should_delete_cookie("", &allowed));
    }

    #[test]
    fn sandbox_dirs_lists_only_site_directories() {
        let root = unique_root("dirs");
        let mut registry = SiteSandboxRegistry::new(root.clone());
        registry.touch("https://example.com").unwrap();
        registry.touch("https://other.example").unwrap();
        let dirs = sandbox_dirs_for_root(&root);
        assert_eq!(dirs.len(), 2);
        assert!(dirs.iter().all(|dir| dir.starts_with(&root)));
        std::fs::remove_dir_all(root).ok();
    }
}
