//! Mode privé / anti-détection. Voir `docs/superpowers/specs/privacy-mode.md`.

pub mod config;
pub mod cookies;
pub mod fingerprint;
pub mod headers;
pub mod network;
pub mod permissions;
pub mod settings;
pub mod tracker;

pub use config::{
    privacy_settings, rotate_profile, PrivacyMode, PrivacySettings, PRIVACY_DATA_DIR,
    PRIVACY_FINGERPRINT,
};
pub use cookies::{
    purge_expired_sandboxes, sandbox_dir_name, sandbox_dirs, should_accept_cookie,
    should_delete_cookie, site_origin, SiteSandbox, SiteSandboxRegistry, PURGE_TICK,
    SANDBOX_IDLE_LIMIT,
};
pub use fingerprint::{fingerprint_script, inject_fingerprint, FINGERPRINT_WORLD};
pub use headers::{
    apply_referer_policy, harden_outgoing_request, harden_request_headers, referer_policy,
    STRIPPED_CLIENT_HINTS,
};
pub use network::{
    detect_vpn, drain_kill_switch_notifications, format_vpn_status, is_internal_host,
    kill_switch_uri_allowed, nordvpn_connected, nordvpn_rotate, notify_kill_switch_blocked,
    provider_for_interface, vpn_interfaces_up, vpn_kill_switch, KillSwitch, VpnStatus,
    MSG_VPN_BLOCKED_BODY, MSG_VPN_BLOCKED_TITLE, VPN_LOST_URI, VPN_OVERLAY_POLL, VPN_RECHECK_TICK,
};
pub use permissions::{describe_permission, permission_denied, permission_origin, PERMISSION_APIS};
pub use settings::{
    clear_previous_session_blocking, create_private_web_context, harden_settings,
    harden_web_context,
};
pub use tracker::{is_tracker_host, TrackerBlocker};

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::privacy::tracker::TrackerBlocker as TrackerBlockerType;

/// Faisceau partagé par tous les onglets. Les drapeaux sont dans des `Cell`
/// pour que `:privacy` et `:vpn` mettent à jour les WebView déjà ouvertes.
#[derive(Clone)]
pub struct PrivacyPolicy {
    pub tracker: Rc<TrackerBlockerType>,
    pub kill_switch: KillSwitch,
    enabled: Rc<Cell<bool>>,
    mode: Rc<Cell<PrivacyMode>>,
    /// Profil tiré pour la session en mode `Rotate`. `None` tant qu'aucune
    /// rotation n'a eu lieu.
    rotated: Rc<RefCell<Option<&'static PrivacySettings>>>,
}

impl PrivacyPolicy {
    pub fn new(enabled: bool, vpn_required: bool) -> Self {
        Self {
            tracker: Rc::new(TrackerBlockerType::load_bundled()),
            kill_switch: KillSwitch::new(vpn_required),
            enabled: Rc::new(Cell::new(enabled)),
            mode: Rc::new(Cell::new(if enabled {
                PrivacyMode::Blend
            } else {
                PrivacyMode::Off
            })),
            rotated: Rc::new(RefCell::new(None)),
        }
    }

    pub fn mode(&self) -> PrivacyMode {
        self.mode.get()
    }

    /// Change le mode. `Rotate` exige NordVPN connecté : sans ça, une
    /// empreinte neuve derrière la même IP se relie à la session précédente.
    /// Retourne `false` si le mode est refusé.
    pub fn set_mode(&self, mode: PrivacyMode) -> bool {
        if mode == PrivacyMode::Rotate && !nordvpn_connected() {
            return false;
        }
        self.mode.set(mode);
        self.enabled.set(mode != PrivacyMode::Off);
        if mode == PrivacyMode::Rotate {
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0);
            *self.rotated.borrow_mut() = Some(rotate_profile(seed));
        }
        true
    }

    /// Profil à servir. `Blend` sert le profil figé. `Rotate` sert le profil
    /// tiré pour la session. `Blank` et `Off` n'ont pas de profil à mentir.
    pub fn profile(&self) -> Option<&'static PrivacySettings> {
        match self.mode() {
            PrivacyMode::Blend => Some(&PRIVACY_FINGERPRINT),
            PrivacyMode::Rotate => *self.rotated.borrow(),
            PrivacyMode::Blank | PrivacyMode::Off => None,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.get()
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.set(enabled);
    }

    /// true si l'URL doit être bloquée (traqueur EasyPrivacy). Inerte si le
    /// paquet privé est off.
    pub fn is_blocked(&self, url: &str) -> bool {
        self.is_enabled() && self.tracker.is_blocked(url)
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
