//! Mode privé / anti-détection. Voir `docs/superpowers/specs/privacy-mode.md`.

pub mod config;
pub mod cookies;
pub mod settings;

pub use config::{privacy_settings, PrivacySettings, PRIVACY_DATA_DIR, PRIVACY_FINGERPRINT};
pub use cookies::{
    purge_expired_sandboxes, sandbox_dir_name, sandbox_dirs, should_accept_cookie,
    should_delete_cookie, site_origin, SiteSandbox, SiteSandboxRegistry, PURGE_TICK,
    SANDBOX_IDLE_LIMIT,
};
pub use settings::{
    clear_previous_session_blocking, create_private_web_context, harden_settings,
    harden_web_context,
};

use std::rc::Rc;

/// Faisceau consommé par `create_webview` : blocage traqueurs + kill-switch.
#[derive(Clone)]
pub struct PrivacyPolicy {
    pub tracker: Rc<TrackerBlocker>,
    pub kill_switch: KillSwitch,
}

impl PrivacyPolicy {
    pub fn new(vpn_required: bool) -> Self {
        Self {
            tracker: Rc::new(TrackerBlocker::load_bundled()),
            kill_switch: KillSwitch::new(vpn_required),
        }
    }

    /// true si l'URL doit être bloquée (traqueur EasyPrivacy).
    pub fn is_blocked(&self, url: &str) -> bool {
        self.tracker.is_blocked(url)
    }

    /// Allowlist existante + kill-switch VPN. Fail-closed : `None` → refus.
    pub fn navigation_allowed(&self, uri: Option<&str>, is_blocked: impl Fn(&str) -> bool) -> bool {
        match uri {
            Some(uri) if self.kill_switch.allow_navigation(uri) => {
                crate::commands::should_allow_navigation(Some(uri), is_blocked)
            }
            _ => false,
        }
    }
}

// TEMPORAIRE (Task 1) — remplacé par privacy::tracker puis privacy::network.
pub struct TrackerBlocker;
impl TrackerBlocker {
    pub fn load_bundled() -> Self {
        Self
    }
    pub fn is_blocked(&self, _url: &str) -> bool {
        false
    }
}

#[derive(Clone)]
pub struct KillSwitch {
    vpn_required: bool,
}
impl KillSwitch {
    pub fn new(vpn_required: bool) -> Self {
        Self { vpn_required }
    }
    pub fn allow_navigation(&self, _uri: &str) -> bool {
        !self.vpn_required
    }
}
