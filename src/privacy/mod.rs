//! Mode privé / anti-détection. Voir `docs/superpowers/specs/privacy-mode.md`.

pub mod config;
pub mod settings;

pub use config::{privacy_settings, PrivacySettings, PRIVACY_DATA_DIR, PRIVACY_FINGERPRINT};
pub use settings::{
    clear_previous_session_blocking, create_private_web_context, harden_settings,
    harden_web_context,
};
