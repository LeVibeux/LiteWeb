use crate::storage::PermissionRecord;

/// Toute surface sensible : refusée sans exception explicite de l'utilisateur.
pub const PERMISSION_APIS: [&str; 22] = [
    "geolocation",
    "notification",
    "camera",
    "microphone",
    "screen",
    "clipboard-read",
    "clipboard-write",
    "usb",
    "bluetooth",
    "hid",
    "sensors",
    "midi",
    "serial",
    "idle-detection",
    "window-placement",
    "font-access",
    "local-font-access",
    "storage-access",
    "local-network-access",
    "pointer-lock",
    "fullscreen",
    "payment",
];

/// Origine normalisée `scheme://host[:port]` — clé du registre.
pub fn permission_origin(url: &str) -> Option<String> {
    crate::privacy::site_origin(url)
}

/// Fail-closed : origine absente, origine non http(s), API inconnue → refus.
pub fn permission_denied(
    origin: Option<&str>,
    api: &str,
    is_granted: impl Fn(&str, &str) -> bool,
) -> bool {
    if !PERMISSION_APIS.contains(&api) {
        return true;
    }
    let Some(raw) = origin else {
        return true;
    };
    let Some(origin) = permission_origin(raw) else {
        return true;
    };
    !is_granted(&origin, api)
}

/// Ligne lisible pour `:permission list`.
pub fn describe_permission(record: &PermissionRecord) -> String {
    let state = if record.granted {
        "accordée"
    } else {
        "refusée"
    };
    format!(
        "• {} — {} [{}] le {}",
        record.origin,
        record.api,
        state,
        record.granted_at.format("%Y-%m-%d %H:%M")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_list_covers_every_sensitive_surface() {
        for api in [
            "geolocation",
            "notification",
            "camera",
            "microphone",
            "screen",
            "clipboard-read",
            "clipboard-write",
            "usb",
            "bluetooth",
            "hid",
            "sensors",
            "midi",
            "serial",
            "idle-detection",
            "window-placement",
            "font-access",
            "local-font-access",
            "storage-access",
            "local-network-access",
            "pointer-lock",
            "fullscreen",
            "payment",
        ] {
            assert!(PERMISSION_APIS.contains(&api), "{api} manque");
        }
    }

    #[test]
    fn everything_is_denied_by_default() {
        assert!(permission_denied(
            Some("https://example.com"),
            "geolocation",
            |_, _| false
        ));
        assert!(permission_denied(
            Some("https://example.com"),
            "camera",
            |_, _| false
        ));
        assert!(permission_denied(
            Some("https://example.com"),
            "notification",
            |_, _| false
        ));
    }

    #[test]
    fn granted_origins_are_allowed_for_that_api_only() {
        let granted = |origin: &str, api: &str| origin == "https://example.com" && api == "camera";
        assert!(!permission_denied(
            Some("https://example.com"),
            "camera",
            granted
        ));
        assert!(permission_denied(
            Some("https://example.com"),
            "microphone",
            granted
        ));
        assert!(permission_denied(
            Some("https://other.example"),
            "camera",
            granted
        ));
    }

    #[test]
    fn missing_origin_or_unknown_api_is_fail_closed() {
        assert!(permission_denied(None, "camera", |_, _| true));
        assert!(permission_denied(Some("about:blank"), "camera", |_, _| {
            true
        }));
        assert!(permission_denied(
            Some("https://example.com"),
            "telepathy",
            |_, _| true
        ));
    }

    #[test]
    fn origin_is_normalised_to_scheme_host_port() {
        assert_eq!(
            permission_origin("https://Example.com:443/a/b?c=d").as_deref(),
            Some("https://example.com")
        );
        assert_eq!(permission_origin("not a url"), None);
        assert_eq!(permission_origin("file:///etc/passwd"), None);
    }
}
