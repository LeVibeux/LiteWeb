#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandAction {
    Open(String),
    Tab(usize),
    TabNew(String),
    TabNext,
    TabPrev,
    Suspend,
    SuspendAll,
    EcoOn,
    EcoOff,
    EcoAggressive,
    EcoUltra,
    BookmarkAdd,
    BookmarkList,
    History,
    DownloadList,
    Unknown(String),
}

pub struct CommandPalette;

impl CommandPalette {
    pub fn parse(input: &str) -> CommandAction {
        let input = input.trim();
        if !input.starts_with(':') {
            return CommandAction::Open(Self::normalize_url(input));
        }

        let cmd = input.trim_start_matches(':').trim();
        let mut parts = cmd.splitn(2, char::is_whitespace);
        let verb = parts.next().unwrap_or("").to_lowercase();
        let arg = parts.next().unwrap_or("").trim();

        match verb.as_str() {
            "open" | "o" => CommandAction::Open(Self::normalize_url(arg)),
            "tab" | "t" => {
                let mut words = arg.split_whitespace();
                let first = words.next().unwrap_or("").to_lowercase();
                match first.as_str() {
                    "" => CommandAction::Unknown(cmd.to_string()),
                    "new" => {
                        let rest: String = words.collect::<Vec<_>>().join(" ");
                        let url = if rest.is_empty() {
                            "about:blank".to_string()
                        } else {
                            Self::normalize_url(&rest)
                        };
                        CommandAction::TabNew(url)
                    }
                    "next" => CommandAction::TabNext,
                    "prev" => CommandAction::TabPrev,
                    _ => {
                        if let Ok(n) = first.parse::<usize>() {
                            CommandAction::Tab(n.saturating_sub(1))
                        } else {
                            CommandAction::Unknown(cmd.to_string())
                        }
                    }
                }
            }
            "suspend" => CommandAction::Suspend,
            "suspend-all" | "suspendall" => CommandAction::SuspendAll,
            "eco" => match arg.to_lowercase().as_str() {
                "on" => CommandAction::EcoOn,
                "off" => CommandAction::EcoOff,
                "aggressive" | "agg" => CommandAction::EcoAggressive,
                "ultra" | "u" => CommandAction::EcoUltra,
                _ => CommandAction::EcoOn,
            },
            "bookmark" | "bm" => match arg.to_lowercase().as_str() {
                "list" | "" => CommandAction::BookmarkList,
                _ => CommandAction::BookmarkAdd,
            },
            "history" | "h" => CommandAction::History,
            "download" | "dl" => CommandAction::DownloadList,
            _ => CommandAction::Unknown(cmd.to_string()),
        }
    }

    fn normalize_url(input: &str) -> String {
        let input = input.trim();
        if input.is_empty() {
            return "about:blank".to_string();
        }
        if input.contains("://") || input.starts_with("about:") {
            input.to_string()
        } else if input.contains('.') && !input.contains(' ') {
            format!("https://{input}")
        } else {
            format!("https://duckduckgo.com/?q={}", encode_query(input))
        }
    }
}

fn encode_query(input: &str) -> String {
    url::form_urlencoded::byte_serialize(input.as_bytes()).collect()
}

pub(crate) fn is_safe_navigation_url(input: &str) -> bool {
    if input == "about:blank" {
        return true;
    }
    if input.bytes().any(|b| b < 0x20 || b == 0x7f) {
        return false;
    }

    let Ok(parsed) = url::Url::parse(input) else {
        return false;
    };
    matches!(parsed.scheme(), "http" | "https")
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && !parsed.cannot_be_a_base()
        && parsed.host_str().is_some_and(is_usable_http_host)
}

fn is_usable_http_host(host: &str) -> bool {
    let host = host.trim_matches(|c| c == '[' || c == ']');
    !host.is_empty() && !host.bytes().all(|b| b == b'.')
}

pub(crate) fn should_allow_navigation(
    uri: Option<&str>,
    is_blocked: impl Fn(&str) -> bool,
) -> bool {
    match uri {
        Some(uri) if is_safe_navigation_url(uri) && !is_blocked(uri) => true,
        _ => false,
    }
}

pub(crate) fn is_bidi_control(ch: char) -> bool {
    matches!(
        ch,
        '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
    )
}

pub(crate) fn sanitize_ui_text(input: &str) -> String {
    input.chars().filter(|ch| !is_bidi_control(*ch)).collect()
}

pub(crate) fn display_navigation_url(input: &str) -> String {
    let cleaned = sanitize_ui_text(input);
    if cleaned == "about:blank" {
        return cleaned;
    }
    match url::Url::parse(&cleaned) {
        Ok(parsed) => parsed.to_string(),
        Err(_) => cleaned,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_open_command() {
        assert_eq!(
            CommandPalette::parse(":open example.com"),
            CommandAction::Open("https://example.com".into())
        );
    }

    #[test]
    fn parses_suspend_all() {
        assert_eq!(
            CommandPalette::parse(":suspend-all"),
            CommandAction::SuspendAll
        );
    }

    #[test]
    fn bare_text_is_search() {
        let action = CommandPalette::parse("rust lang");
        match action {
            CommandAction::Open(url) => assert!(url.contains("duckduckgo.com")),
            _ => panic!("expected open/search"),
        }
    }

    #[test]
    fn parses_tab_new_blank() {
        assert_eq!(
            CommandPalette::parse(":tab new"),
            CommandAction::TabNew("about:blank".into())
        );
    }

    #[test]
    fn parses_tab_new_url() {
        assert_eq!(
            CommandPalette::parse(":tab new example.com"),
            CommandAction::TabNew("https://example.com".into())
        );
    }

    #[test]
    fn parses_tab_next_prev() {
        assert_eq!(CommandPalette::parse(":tab next"), CommandAction::TabNext);
        assert_eq!(CommandPalette::parse(":tab prev"), CommandAction::TabPrev);
    }

    #[test]
    fn parses_t_alias_for_tab_new() {
        assert_eq!(
            CommandPalette::parse(":t new"),
            CommandAction::TabNew("about:blank".into())
        );
    }

    #[test]
    fn parses_tab_index_still_works() {
        assert_eq!(CommandPalette::parse(":tab 2"), CommandAction::Tab(1));
    }

    #[test]
    fn parses_eco_ultra() {
        assert_eq!(CommandPalette::parse(":eco ultra"), CommandAction::EcoUltra);
        assert_eq!(CommandPalette::parse(":eco u"), CommandAction::EcoUltra);
    }

    #[test]
    fn tab_empty_is_unknown() {
        match CommandPalette::parse(":tab") {
            CommandAction::Unknown(_) => {}
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn search_query_is_percent_encoded() {
        assert_eq!(
            CommandPalette::parse("rust & sécurité"),
            CommandAction::Open("https://duckduckgo.com/?q=rust+%26+s%C3%A9curit%C3%A9".into())
        );
    }

    #[test]
    fn navigation_allowlist_rejects_local_and_active_schemes() {
        assert!(is_safe_navigation_url("https://example.com/path"));
        assert!(is_safe_navigation_url("http://localhost:8080"));
        assert!(is_safe_navigation_url("about:blank"));
        assert!(!is_safe_navigation_url("file:///etc/passwd"));
        assert!(!is_safe_navigation_url("javascript:alert(1)"));
        assert!(!is_safe_navigation_url("data:text/html,hello"));
        assert!(!is_safe_navigation_url("about:config"));
        assert!(!is_safe_navigation_url("https://user:secret@example.com"));
    }

    #[test]
    fn navigation_allowlist_rejects_dot_only_hosts_and_non_web_schemes() {
        assert!(!is_safe_navigation_url("http://."));
        assert!(!is_safe_navigation_url("http://.."));
        assert!(!is_safe_navigation_url("https://."));
        assert!(!is_safe_navigation_url("ws://example.com"));
        assert!(!is_safe_navigation_url("wss://example.com"));
        assert!(!is_safe_navigation_url("ftp://ftp.example.com"));
        assert!(!is_safe_navigation_url("blob:https://example.com/uuid"));
        assert!(!is_safe_navigation_url("view-source:https://example.com"));
        assert!(!is_safe_navigation_url(
            "javascript://https://example.com/%0aalert(1)"
        ));
        assert!(!is_safe_navigation_url(
            "intent://scan/#Intent;scheme=http;end"
        ));
        assert!(!is_safe_navigation_url("https://user@example.com"));
        assert!(!is_safe_navigation_url("https://example.com@evil.com"));
    }

    #[test]
    fn navigation_policy_fails_closed_without_a_uri() {
        assert!(!should_allow_navigation(None, |_| false));
        assert!(!should_allow_navigation(
            Some("https://ads.example"),
            |_| { true }
        ));
        assert!(should_allow_navigation(Some("https://example.com"), |_| {
            false
        }));
        assert!(!should_allow_navigation(
            Some("javascript:alert(1)"),
            |_| false
        ));
    }

    #[test]
    fn display_url_uses_punycode_for_idn_hosts() {
        let unicode = "https://аpple.com/login";
        let shown = display_navigation_url(unicode);
        assert!(
            shown.contains("xn--"),
            "IDN hosts must be shown as punycode, got {shown}"
        );
        assert!(
            !shown.contains('а'),
            "Cyrillic lookalike must not stay in the address bar, got {shown}"
        );
    }

    #[test]
    fn display_url_strips_bidi_overrides() {
        let spoofed = format!("https://example.com/{}\u{202E}moc.evil", "docs/");
        let shown = display_navigation_url(&spoofed);
        assert!(
            !shown.chars().any(is_bidi_control),
            "bidi overrides must not reach the address bar, got {shown:?}"
        );
        assert!(shown.contains("example.com"));
    }

    #[test]
    fn ui_text_strips_bidi_overrides_from_page_titles() {
        let title = format!("Banque {}\u{202E}evil", "Nationale");
        let shown = sanitize_ui_text(&title);
        assert!(!shown.chars().any(is_bidi_control));
        assert!(shown.contains("Banque"));
        assert!(shown.contains("evil"));
    }
}
