use soup::{MessageHeaders, MessageHeadersType};
use url::Url;
use webkit2gtk::{URIRequest, URIRequestExt};

use crate::privacy::PRIVACY_FINGERPRINT;

/// Tous les en-têtes de fingerprinting supprimés (spec, Axe 3).
/// `Sec-Fetch-*` et `Origin` restent gérés par WebKit : requis pour CORS.
/// 18 noms : le plan disait `[&str; 16]` mais la liste du spec + le test
/// `client_hint_list_covers_the_spec` en exigent 18 (dont `Sec-CH-Width`).
pub const STRIPPED_CLIENT_HINTS: [&str; 18] = [
    "Sec-CH-UA",
    "Sec-CH-UA-Mobile",
    "Sec-CH-UA-Platform",
    "Sec-CH-UA-Platform-Version",
    "Sec-CH-UA-Arch",
    "Sec-CH-UA-Model",
    "Sec-CH-UA-Full-Version",
    "Sec-CH-UA-Full-Version-List",
    "Sec-CH-UA-Bitness",
    "Sec-CH-UA-Wow64",
    "Sec-CH-Prefers-Color-Scheme",
    "Sec-CH-Prefers-Reduced-Motion",
    "Sec-CH-Viewport-Width",
    "Sec-CH-Viewport-Height",
    "Sec-CH-Width",
    "Sec-CH-DPR",
    "Sec-CH-Device-Memory",
    "Sec-CH-Downlink",
];

/// Politique `Referer` : cross-origin → aucun `Referer` ; same-origin →
/// réduit à l'origine. Fail-closed : page inconnue ou non parsable → `None`.
pub fn referer_policy(page_uri: Option<&str>, request_uri: &str) -> Option<String> {
    let page = Url::parse(page_uri?).ok()?;
    let request = Url::parse(request_uri).ok()?;
    if !matches!(page.scheme(), "http" | "https") || !matches!(request.scheme(), "http" | "https") {
        return None;
    }
    let page_origin = origin_of(&page)?;
    if page_origin != origin_of(&request)? {
        return None;
    }
    Some(format!("{page_origin}/"))
}

/// Origine sans port par défaut. `Url::port()` renvoie `None` pour 443/80,
/// donc `https://example.com` et `https://example.com:443` sont same-origin
/// et l'origine émise est `https://example.com` (pas `:443`).
fn origin_of(url: &Url) -> Option<String> {
    let host = url.host_str()?.to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    match url.port() {
        Some(port) => Some(format!("{}://{}:{}", url.scheme(), host, port)),
        None => Some(format!("{}://{}", url.scheme(), host)),
    }
}

/// Applique la politique `Referer` : `Some(origin)` → `replace`, `None` → `remove`.
pub fn apply_referer_policy(headers: &MessageHeaders, referer: Option<&str>) {
    match referer {
        Some(value) => headers.replace("Referer", value),
        None => headers.remove("Referer"),
    }
}

/// DNT + Accept-Language figé + suppression de tous les `Sec-CH-*`.
pub fn harden_request_headers(headers: &MessageHeaders, page_uri: Option<&str>, request_uri: &str) {
    for name in STRIPPED_CLIENT_HINTS {
        headers.remove(name);
    }
    headers.replace("DNT", PRIVACY_FINGERPRINT.dnt);
    headers.replace("Accept-Language", PRIVACY_FINGERPRINT.accept_language);
    let referer = referer_policy(page_uri, request_uri);
    apply_referer_policy(headers, referer.as_deref());
}

/// Point d'entrée utilisé par `resource-load-started`.
pub fn harden_outgoing_request(request: &URIRequest, page_uri: Option<&str>) {
    let Some(headers) = request.http_headers() else {
        return;
    };
    let request_uri = request.uri().map(|uri| uri.to_string()).unwrap_or_default();
    harden_request_headers(&headers, page_uri, &request_uri);
}

#[cfg(test)]
mod tests {
    use super::*;
    use webkit2gtk::URIRequestExt;

    fn headers() -> MessageHeaders {
        gtk::init().expect("gtk init");
        MessageHeaders::new(MessageHeadersType::Request)
    }

    #[test]
    fn cross_origin_requests_carry_no_referer_at_all() {
        assert_eq!(
            referer_policy(
                Some("https://example.com/secret/path"),
                "https://tracker.test/p.gif"
            ),
            None
        );
    }

    #[test]
    fn same_origin_referer_is_reduced_to_the_origin() {
        assert_eq!(
            referer_policy(Some("https://example.com/a/b?c=d"), "https://example.com/e").as_deref(),
            Some("https://example.com/")
        );
        assert_eq!(
            referer_policy(Some("https://example.com/a"), "https://example.com:443/b").as_deref(),
            Some("https://example.com/")
        );
    }

    #[test]
    fn port_and_scheme_changes_are_cross_origin() {
        assert_eq!(
            referer_policy(Some("https://example.com/a"), "http://example.com/b"),
            None,
            "changement de schéma = cross-origin"
        );
        assert_eq!(
            referer_policy(Some("http://localhost:8080/a"), "http://localhost:3000/b"),
            None
        );
        assert_eq!(
            referer_policy(Some("http://localhost:8080/a"), "http://localhost:8080/b").as_deref(),
            Some("http://localhost:8080/")
        );
    }

    #[test]
    fn unparsable_page_uri_is_fail_closed() {
        assert_eq!(
            referer_policy(Some("not a url"), "https://example.com/"),
            None
        );
        assert_eq!(
            referer_policy(Some("about:blank"), "https://example.com/"),
            None
        );
        assert_eq!(referer_policy(None, "https://example.com/"), None);
    }

    #[test]
    #[ignore = "MessageHeaders abort le process quand plusieurs tests GTK tournent"]
    fn all_client_hints_are_stripped() {
        let h = headers();
        for name in STRIPPED_CLIENT_HINTS {
            h.append(name, "leaked");
        }
        h.append("Sec-Fetch-Mode", "navigate");
        h.append("Origin", "https://example.com");
        harden_request_headers(&h, Some("https://example.com/"), "https://example.com/");
        for name in STRIPPED_CLIENT_HINTS {
            assert!(h.one(name).is_none(), "{name} doit être supprimé");
        }
        assert_eq!(
            h.one("Sec-Fetch-Mode").as_deref(),
            Some("navigate"),
            "Sec-Fetch-* reste géré par WebKit (requis pour CORS)"
        );
        assert_eq!(h.one("Origin").as_deref(), Some("https://example.com"));
    }

    #[test]
    fn client_hint_list_covers_the_spec() {
        for name in [
            "Sec-CH-UA",
            "Sec-CH-UA-Mobile",
            "Sec-CH-UA-Platform",
            "Sec-CH-UA-Platform-Version",
            "Sec-CH-UA-Arch",
            "Sec-CH-UA-Model",
            "Sec-CH-UA-Full-Version",
            "Sec-CH-UA-Full-Version-List",
            "Sec-CH-UA-Bitness",
            "Sec-CH-UA-Wow64",
            "Sec-CH-Prefers-Color-Scheme",
            "Sec-CH-Prefers-Reduced-Motion",
            "Sec-CH-Viewport-Width",
            "Sec-CH-Viewport-Height",
            "Sec-CH-DPR",
            "Sec-CH-Device-Memory",
            "Sec-CH-Downlink",
            "Sec-CH-Width",
        ] {
            assert!(
                STRIPPED_CLIENT_HINTS.contains(&name),
                "{name} manque dans STRIPPED_CLIENT_HINTS"
            );
        }
    }

    #[test]
    #[ignore = "MessageHeaders abort le process quand plusieurs tests GTK tournent"]
    fn hardening_sets_dnt_and_the_frozen_accept_language() {
        let h = headers();
        h.append("Accept-Language", "en-US,en;q=0.5");
        h.append("Referer", "https://example.com/secret/deep/path");
        h.append("Sec-CH-UA", "\"Chromium\";v=\"127\"");
        harden_request_headers(
            &h,
            Some("https://example.com/secret/deep/path"),
            "https://example.com/next",
        );
        assert_eq!(h.one("DNT").as_deref(), Some("1"));
        assert_eq!(
            h.one("Accept-Language").as_deref(),
            Some(PRIVACY_FINGERPRINT.accept_language)
        );
        assert_eq!(h.one("Referer").as_deref(), Some("https://example.com/"));
        assert!(h.one("Sec-CH-UA").is_none());
        assert_eq!(
            h.list("DNT").map(|v| v.split(',').count()).unwrap_or(0),
            1,
            "DNT ne doit pas être dupliqué"
        );
    }

    #[test]
    #[ignore = "MessageHeaders abort le process quand plusieurs tests GTK tournent"]
    fn cross_origin_request_loses_its_referer_header() {
        let h = headers();
        h.append("Referer", "https://example.com/secret/deep/path");
        harden_request_headers(
            &h,
            Some("https://example.com/page"),
            "https://cdn.other.test/x.js",
        );
        assert!(h.one("Referer").is_none());
        assert_eq!(h.one("DNT").as_deref(), Some("1"));
    }

    #[test]
    #[ignore = "URIRequest segfault sans display GTK"]
    fn uri_request_headers_are_hardened_end_to_end() {
        gtk::init().expect("gtk init");
        let request = webkit2gtk::URIRequest::new("https://example.com/page");
        if let Some(h) = request.http_headers() {
            h.append("Sec-CH-UA-Platform", "\"Linux\"");
            h.append("Referer", "https://tracker.test/previous");
            harden_outgoing_request(&request, Some("https://example.com/"));
            assert!(h.one("Sec-CH-UA-Platform").is_none());
            assert!(h.one("Referer").is_none(), "cross-origin → pas de Referer");
            assert_eq!(h.one("DNT").as_deref(), Some("1"));
        }
    }
}
