#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandAction {
    Open(String),
    Tab(usize),
    Suspend,
    SuspendAll,
    EcoOn,
    EcoOff,
    EcoAggressive,
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
                let n = arg.parse::<usize>().unwrap_or(1);
                CommandAction::Tab(n.saturating_sub(1))
            }
            "suspend" => CommandAction::Suspend,
            "suspend-all" | "suspendall" => CommandAction::SuspendAll,
            "eco" => match arg.to_lowercase().as_str() {
                "on" => CommandAction::EcoOn,
                "off" => CommandAction::EcoOff,
                "aggressive" | "agg" => CommandAction::EcoAggressive,
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
            format!(
                "https://duckduckgo.com/?q={}",
                urlencoding_like(input)
            )
        }
    }
}

fn urlencoding_like(s: &str) -> String {
    s.replace(' ', "+")
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
        assert_eq!(CommandPalette::parse(":suspend-all"), CommandAction::SuspendAll);
    }

    #[test]
    fn bare_text_is_search() {
        let action = CommandPalette::parse("rust lang");
        match action {
            CommandAction::Open(url) => assert!(url.contains("duckduckgo.com")),
            _ => panic!("expected open/search"),
        }
    }
}
