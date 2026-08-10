use std::collections::HashSet;

#[derive(Debug, Clone)]
struct FilterRule {
    pattern: String,
    is_domain: bool,
    is_exception: bool,
}

pub struct FilterEngine {
    rules: Vec<FilterRule>,
    blocked_domains: HashSet<String>,
    exception_domains: HashSet<String>,
}

impl FilterEngine {
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            blocked_domains: HashSet::new(),
            exception_domains: HashSet::new(),
        }
    }

    pub fn load_bundled(&mut self) {
        self.load_from_str(include_str!("../../assets/filters.txt"));
    }

    pub fn load_from_str(&mut self, content: &str) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('!') || line.starts_with('[') {
                continue;
            }
            if let Some(rule) = Self::parse_line(line) {
                if rule.is_domain {
                    let domain = rule
                        .pattern
                        .trim_start_matches("||")
                        .trim_end_matches('^')
                        .to_string();
                    if rule.is_exception {
                        self.exception_domains.insert(domain);
                    } else {
                        self.blocked_domains.insert(domain);
                    }
                }
                self.rules.push(rule);
            }
        }
    }

    fn parse_line(line: &str) -> Option<FilterRule> {
        let (is_exception, rest) = if let Some(rest) = line.strip_prefix("@@") {
            (true, rest.trim())
        } else {
            (false, line)
        };

        if rest.starts_with("||") {
            return Some(FilterRule {
                pattern: rest.to_string(),
                is_domain: true,
                is_exception,
            });
        }

        if rest.starts_with('|') && rest.ends_with('|') && rest.len() > 2 {
            return Some(FilterRule {
                pattern: rest[1..rest.len() - 1].to_string(),
                is_domain: false,
                is_exception,
            });
        }

        None
    }

    pub fn should_block(&self, url: &str) -> bool {
        if let Ok(parsed) = url::Url::parse(url) {
            if let Some(host) = parsed.host_str() {
                if self
                    .exception_domains
                    .iter()
                    .any(|domain| domain_matches(host, domain))
                {
                    return false;
                }
                for domain in &self.blocked_domains {
                    if domain_matches(host, domain) {
                        return true;
                    }
                }
            }
        }

        let matching_rules = self
            .rules
            .iter()
            .filter(|rule| !rule.is_domain && rule.pattern.len() > 3 && url.contains(&rule.pattern))
            .collect::<Vec<_>>();
        if matching_rules.iter().any(|rule| rule.is_exception) {
            return false;
        }
        matching_rules.iter().any(|rule| !rule.is_exception)
    }

    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
}

fn domain_matches(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{domain}"))
}

pub struct Blocker {
    engine: FilterEngine,
    blocked_count: std::cell::RefCell<u64>,
}

impl Blocker {
    pub fn new() -> Self {
        let mut engine = FilterEngine::new();
        engine.load_bundled();
        Self {
            engine,
            blocked_count: std::cell::RefCell::new(0),
        }
    }

    pub fn check(&self, url: &str) -> bool {
        let block = self.engine.should_block(url);
        if block {
            *self.blocked_count.borrow_mut() += 1;
        }
        block
    }

    pub fn blocked_count(&self) -> u64 {
        *self.blocked_count.borrow()
    }

    pub fn rule_count(&self) -> usize {
        self.engine.rule_count()
    }
}

impl Default for Blocker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_known_tracker_domain() {
        let mut engine = FilterEngine::new();
        engine.load_from_str("||doubleclick.net^");
        assert!(engine.should_block("https://ad.doubleclick.net/click"));
    }

    #[test]
    fn allows_clean_url() {
        let mut engine = FilterEngine::new();
        engine.load_from_str("||doubleclick.net^");
        assert!(!engine.should_block("https://example.com/page"));
    }

    #[test]
    fn domain_exception_wins_regardless_of_rule_order() {
        for rules in [
            "||example.com^\n@@||allowed.example.com^",
            "@@||allowed.example.com^\n||example.com^",
        ] {
            let mut engine = FilterEngine::new();
            engine.load_from_str(rules);
            assert!(engine.should_block("https://ads.example.com/banner"));
            assert!(!engine.should_block("https://allowed.example.com/page"));
        }
    }

    #[test]
    fn exact_exception_wins_regardless_of_rule_order() {
        for rules in [
            "|tracker.js|\n@@|tracker.js|",
            "@@|tracker.js|\n|tracker.js|",
        ] {
            let mut engine = FilterEngine::new();
            engine.load_from_str(rules);
            assert!(!engine.should_block("https://cdn.example/tracker.js"));
        }
    }
}
