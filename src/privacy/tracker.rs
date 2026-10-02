use std::cell::Cell;
use std::collections::HashSet;

use url::Url;

/// Chargeur + matcher EasyPrivacy. Seules les règles **de domaine** sont
/// supportées (`||host^`, `@@||host^`, `|https://host/path|`) : les règles de
/// chemin ou d'option sont réduites à leur hôte à l'import.
///
/// `Cell<u64>` est utilisé pour le compteur de hits : le navigateur est
/// mono-thread (GTK main loop), donc pas de concurrence. On force `Send+Sync`
/// pour `OnceLock` — le `Cell` n'est jamais accédé depuis un autre thread.
pub struct TrackerBlocker {
    blocked: HashSet<String>,
    exceptions: HashSet<String>,
    rules: usize,
    hits: Cell<u64>,
}

// Safety: le navigateur est mono-thread (GTK). Le Cell n'est jamais partagé.
unsafe impl Send for TrackerBlocker {}
unsafe impl Sync for TrackerBlocker {}

impl TrackerBlocker {
    pub fn load_bundled() -> Self {
        Self::from_str(include_str!("../../assets/privacy/easyprivacy.txt"))
    }

    pub fn from_str(content: &str) -> Self {
        let mut blocked = HashSet::new();
        let mut exceptions = HashSet::new();
        let mut rules = 0usize;
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty()
                || line.starts_with('!')
                || line.starts_with('#')
                || line.starts_with('[')
            {
                continue;
            }
            let (is_exception, rest) = match line.strip_prefix("@@") {
                Some(rest) => (true, rest),
                None => (false, line),
            };
            let Some(host) = host_from_rule(rest) else {
                continue;
            };
            rules += 1;
            if is_exception {
                exceptions.insert(host);
            } else {
                blocked.insert(host);
            }
        }
        Self {
            blocked,
            exceptions,
            rules,
            hits: Cell::new(0),
        }
    }

    pub fn is_blocked(&self, url: &str) -> bool {
        let Ok(parsed) = Url::parse(url) else {
            return false;
        };
        if !matches!(parsed.scheme(), "http" | "https") {
            return false;
        }
        let Some(host) = parsed.host_str() else {
            return false;
        };
        let blocked = self.blocks_host(host);
        if blocked {
            self.hits.set(self.hits.get() + 1);
        }
        blocked
    }

    pub fn blocks_host(&self, host: &str) -> bool {
        let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
        if host.is_empty() {
            return false;
        }
        if matches_domain(&self.exceptions, &host) {
            return false;
        }
        matches_domain(&self.blocked, &host)
    }

    pub fn rule_count(&self) -> usize {
        self.rules
    }

    pub fn hit_count(&self) -> u64 {
        self.hits.get()
    }
}

impl Default for TrackerBlocker {
    fn default() -> Self {
        Self::load_bundled()
    }
}

/// Hôte seul, utile pour la politique cookies et le registre de permissions.
pub fn is_tracker_host(host: &str) -> bool {
    static BUNDLED: std::sync::OnceLock<TrackerBlocker> = std::sync::OnceLock::new();
    BUNDLED
        .get_or_init(TrackerBlocker::load_bundled)
        .blocks_host(host)
}

fn matches_domain(domains: &HashSet<String>, host: &str) -> bool {
    domains.iter().any(|domain| {
        host == domain
            || host.ends_with(domain.as_str())
                && host.as_bytes()[host.len() - domain.len() - 1] == b'.'
    })
}

/// Extrait l'hôte d'une règle EasyPrivacy : retire `||`, `^`, les ancres `|`,
/// le schéma et tout chemin.
fn host_from_rule(rule: &str) -> Option<String> {
    let rule = rule
        .trim()
        .trim_start_matches("||")
        .trim_start_matches('|')
        .trim_end_matches('|');
    let without_options = rule.split('$').next().unwrap_or(rule);
    let without_anchor = without_options.trim_end_matches('^');
    let authority = match without_anchor.find("://") {
        Some(index) => &without_anchor[index + 3..],
        None => without_anchor,
    };
    let host = authority.split(['/', '?', '#']).next().unwrap_or("").trim();
    let host = host.rsplit('@').next().unwrap_or("").trim();
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() || host.contains('/') || host.contains(' ') {
        return None;
    }
    Some(host)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
! commentaire EasyPrivacy
[Adblock Plus 2.0]
# commentaire shell

||doubleclick.net^
||analytics.example.com^
@@||allowed.example.com^
||TRACKER.IO^
|https://exact.example/beacon.js|
||pinterest.com/ct/^
";

    #[test]
    fn matches_exact_host_and_subdomains() {
        let blocker = TrackerBlocker::from_str(SAMPLE);
        assert!(blocker.blocks_host("doubleclick.net"));
        assert!(blocker.blocks_host("ad.doubleclick.net"));
        assert!(blocker.blocks_host("a.b.ad.doubleclick.net"));
        assert!(blocker.blocks_host("analytics.example.com"));
    }

    #[test]
    fn does_not_match_lookalike_or_parent_hosts() {
        let blocker = TrackerBlocker::from_str(SAMPLE);
        assert!(!blocker.blocks_host("notdoubleclick.net"));
        assert!(!blocker.blocks_host("doubleclick.net.evil.com"));
        assert!(!blocker.blocks_host("example.com"));
        assert!(!blocker.blocks_host(""));
    }

    #[test]
    fn host_matching_is_case_insensitive() {
        let blocker = TrackerBlocker::from_str(SAMPLE);
        assert!(blocker.blocks_host("TRACKER.IO"));
        assert!(blocker.blocks_host("tracker.io"));
        assert!(blocker.blocks_host("Tracker.Io"));
    }

    #[test]
    fn comments_and_empty_lines_are_ignored() {
        let blocker = TrackerBlocker::from_str(SAMPLE);
        assert_eq!(blocker.rule_count(), 6);
    }

    #[test]
    fn path_rules_are_reduced_to_their_host() {
        let blocker =
            TrackerBlocker::from_str("||pinterest.com/ct/^\n|https://exact.example/beacon.js|");
        assert!(blocker.blocks_host("pinterest.com"));
        assert!(blocker.blocks_host("exact.example"));
    }

    #[test]
    fn exceptions_win_over_blocks() {
        let blocker = TrackerBlocker::from_str(SAMPLE);
        assert!(blocker.blocks_host("analytics.example.com"));
        assert!(!blocker.blocks_host("allowed.example.com"));
        assert!(!blocker.blocks_host("sub.allowed.example.com"));
    }

    #[test]
    fn is_blocked_uses_the_url_host_only() {
        let blocker = TrackerBlocker::from_str(SAMPLE);
        assert!(blocker.is_blocked("https://ad.doubleclick.net/click?id=1"));
        assert!(!blocker.is_blocked("https://example.com/page"));
        assert!(!blocker.is_blocked("about:blank"));
        assert!(!blocker.is_blocked("not a url"));
        assert!(!blocker.is_blocked("https://***@allowed.example.com/"));
    }

    #[test]
    fn hits_are_counted() {
        let blocker = TrackerBlocker::from_str(SAMPLE);
        assert_eq!(blocker.hit_count(), 0);
        blocker.is_blocked("https://ad.doubleclick.net/x");
        blocker.is_blocked("https://ad.doubleclick.net/y");
        assert_eq!(blocker.hit_count(), 2);
    }

    #[test]
    fn bundled_list_blocks_the_reference_trackers() {
        let blocker = TrackerBlocker::load_bundled();
        assert!(
            blocker.rule_count() >= 50,
            "liste EasyPrivacy embarquée trop petite"
        );
        for host in [
            "doubleclick.net",
            "www.google-analytics.com",
            "googletagmanager.com",
            "connect.facebook.net",
            "scorecardresearch.com",
            "cdn.taboola.com",
            "static.hotjar.com",
            "api.segment.io",
            "cdn.amplitude.com",
            "sentry.io",
        ] {
            assert!(blocker.blocks_host(host), "{host} doit être bloqué");
        }
        for host in [
            "example.com",
            "duckduckgo.com",
            "localhost",
            "wikipedia.org",
        ] {
            assert!(!blocker.blocks_host(host), "{host} ne doit pas être bloqué");
        }
    }

    #[test]
    fn bundled_list_loads_fast() {
        let started = std::time::Instant::now();
        let blocker = TrackerBlocker::load_bundled();
        assert!(blocker.rule_count() > 0);
        assert!(
            started.elapsed() < std::time::Duration::from_millis(100),
            "chargement EasyPrivacy trop lent: {:?}",
            started.elapsed()
        );
    }
}
