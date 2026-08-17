use std::path::Path;
use std::rc::Rc;

use glib::Cast;
use webkit2gtk::{
    AuthenticationRequestExt, CacheModel, CookieAcceptPolicy, CookieManagerExt, DownloadExt,
    FileChooserRequestExt, HardwareAccelerationPolicy, NavigationPolicyDecision,
    NavigationPolicyDecisionExt, NotificationExt, PermissionRequestExt, PolicyDecisionExt,
    PolicyDecisionType, ProcessModel, ResponsePolicyDecision, ResponsePolicyDecisionExt,
    SettingsExt, TLSErrorsPolicy, URIRequestExt, UserContentInjectedFrames, UserContentManager,
    UserContentManagerExt, UserStyleLevel, UserStyleSheet, WebContext, WebContextExt, WebView,
    WebViewExt, WebViewExtManual, WebsiteDataManager,
};

use crate::adblock::Blocker;
use crate::commands::should_allow_navigation;
use crate::energy::WebViewPolicy;

pub const ARCHAIC_STYLESHEET: &str = r#"
html, body { background: #f4f1e8 !important; color: #1a1a1a !important; overflow-y: auto !important; height: auto !important; }
* {
  animation: none !important;
  animation-duration: 0s !important;
  transition: none !important;
  scroll-behavior: auto !important;
  backdrop-filter: none !important;
  box-shadow: none !important;
  text-shadow: none !important;
}
video, audio, canvas, iframe, svg, [role="dialog"] { display: none !important; }
nav, aside, footer, header, [role="navigation"], [role="banner"], [role="complementary"] {
  display: none !important;
}
img, picture, source { display: none !important; }
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppliedEngineSwitches {
    pub javascript: bool,
    pub images: bool,
    pub media: bool,
    pub hardware_acceleration: bool,
}

pub type NewTabHandler = Rc<dyn Fn(String)>;

pub fn engine_switches(policy: WebViewPolicy) -> AppliedEngineSwitches {
    AppliedEngineSwitches {
        javascript: policy.javascript,
        images: policy.auto_load_images,
        media: policy.media,
        hardware_acceleration: policy.hardware_acceleration,
    }
}

pub fn create_user_content_manager() -> UserContentManager {
    UserContentManager::new()
}

pub fn apply_webview_policy(webview: &WebView, policy: WebViewPolicy) {
    let Some(settings) = webview.settings() else {
        return;
    };
    settings.set_enable_javascript(policy.javascript);
    settings.set_enable_javascript_markup(policy.javascript_markup);
    settings.set_auto_load_images(policy.auto_load_images);
    settings.set_enable_media(policy.media);
    settings.set_enable_mediasource(policy.media);
    settings.set_enable_media_stream(policy.media);
    settings.set_enable_webaudio(policy.webaudio);
    settings.set_enable_webgl(policy.webgl);
    settings.set_enable_webrtc(policy.webrtc);
    settings.set_enable_html5_local_storage(policy.html5_local_storage);
    settings.set_enable_html5_database(policy.html5_database);
    settings.set_enable_smooth_scrolling(policy.smooth_scrolling);
    settings.set_hardware_acceleration_policy(if policy.hardware_acceleration {
        HardwareAccelerationPolicy::OnDemand
    } else {
        HardwareAccelerationPolicy::Never
    });
}

pub fn set_archaic_stylesheet(content: &UserContentManager, enabled: bool) {
    content.remove_all_style_sheets();
    if enabled {
        let sheet = UserStyleSheet::new(
            ARCHAIC_STYLESHEET,
            UserContentInjectedFrames::AllFrames,
            UserStyleLevel::User,
            &[],
            &[],
        );
        content.add_style_sheet(&sheet);
    }
}

#[allow(deprecated)]
pub fn create_webview(
    context: &WebContext,
    blocker: Rc<Blocker>,
    content: &UserContentManager,
    policy: WebViewPolicy,
    on_new_tab: NewTabHandler,
) -> WebView {
    let webview = WebView::new_with_context_and_user_content_manager(context, content);
    apply_webview_policy(&webview, policy);

    if let Some(settings) = webview.settings() {
        settings.set_enable_page_cache(false);
        settings.set_enable_offline_web_application_cache(false);
        settings.set_enable_dns_prefetching(false);
        settings.set_enable_developer_extras(false);
        settings.set_media_playback_requires_user_gesture(true);
        settings.set_javascript_can_open_windows_automatically(false);
        settings.set_javascript_can_access_clipboard(false);
        settings.set_allow_file_access_from_file_urls(false);
        settings.set_allow_universal_access_from_file_urls(false);
        settings.set_allow_top_navigation_to_data_urls(false);
        settings.set_allow_modal_dialogs(false);
        settings.set_enable_fullscreen(false);
        settings.set_enable_hyperlink_auditing(false);
        settings.set_enable_java(false);
        settings.set_enable_plugins(false);
        settings.set_enable_encrypted_media(false);
        settings.set_enable_mock_capture_devices(false);
        settings.set_enable_write_console_messages_to_stdout(false);
        settings.set_enable_xss_auditor(true);
    }

    webview.connect_permission_request(|_, request| {
        request.deny();
        true
    });
    webview.connect_run_file_chooser(|_, request| {
        request.cancel();
        true
    });
    webview.connect_show_notification(|_, notification| {
        notification.close();
        true
    });
    webview.connect_create(|_, _| None);
    webview.connect_enter_fullscreen(|_| true);
    webview.connect_authenticate(|_, request| {
        request.cancel();
        true
    });

    let blocker_policy = blocker.clone();
    webview.connect_decide_policy(move |_view, decision, decision_type| {
        match decision_type {
            PolicyDecisionType::NavigationAction => {
                let uri = decision
                    .dynamic_cast_ref::<NavigationPolicyDecision>()
                    .and_then(|d| d.navigation_action())
                    .and_then(|action| action.request())
                    .and_then(|r| r.uri());
                if should_allow_navigation(uri.as_deref(), |candidate| {
                    blocker_policy.check(candidate)
                }) {
                    false
                } else {
                    decision.ignore();
                    true
                }
            }
            PolicyDecisionType::NewWindowAction => {
                // Never let WebKit spawn an unmanaged view (no policy handlers).
                let uri = decision
                    .dynamic_cast_ref::<NavigationPolicyDecision>()
                    .and_then(|d| d.navigation_action())
                    .and_then(|action| action.request())
                    .and_then(|r| r.uri());
                if should_allow_navigation(uri.as_deref(), |candidate| {
                    blocker_policy.check(candidate)
                }) {
                    if let Some(uri) = uri {
                        let on_new_tab = on_new_tab.clone();
                        let url = uri.to_string();
                        glib::idle_add_local_once(move || on_new_tab(url));
                    }
                }
                decision.ignore();
                true
            }
            PolicyDecisionType::Response => {
                let Some(response) = decision.dynamic_cast_ref::<ResponsePolicyDecision>() else {
                    decision.ignore();
                    return true;
                };
                let uri = response.request().and_then(|request| request.uri());
                if !should_allow_navigation(uri.as_deref(), |candidate| {
                    blocker_policy.check(candidate)
                }) || !response.is_mime_type_supported()
                {
                    decision.ignore();
                    return true;
                }
                false
            }
            _ => {
                decision.ignore();
                true
            }
        }
    });

    webview
}

#[allow(deprecated)]
pub fn create_web_context() -> WebContext {
    let data_manager = private_data_manager();
    let context = WebContext::with_website_data_manager(&data_manager);
    context.set_cache_model(CacheModel::DocumentViewer);
    context.set_process_model(ProcessModel::MultipleSecondaryProcesses);
    context.set_sandbox_enabled(true);
    context.set_tls_errors_policy(TLSErrorsPolicy::Fail);
    context.set_automation_allowed(false);
    if let Some(cookies) = context.cookie_manager() {
        cookies.set_accept_policy(CookieAcceptPolicy::NoThirdParty);
    }
    context.connect_download_started(|_, download| {
        download.cancel();
    });
    context
}

fn private_data_manager() -> WebsiteDataManager {
    let data_dir = dirs::data_dir().map(|path| path.join("liteweb").join("webkit"));
    let cache_dir = dirs::cache_dir().map(|path| path.join("liteweb").join("webkit"));

    match (data_dir, cache_dir) {
        (Some(data_dir), Some(cache_dir))
            if prepare_private_dir(&data_dir).is_ok()
                && prepare_private_dir(&cache_dir).is_ok() =>
        {
            WebsiteDataManager::builder()
                .base_data_directory(data_dir.to_string_lossy().as_ref())
                .base_cache_directory(cache_dir.to_string_lossy().as_ref())
                .build()
        }
        _ => {
            eprintln!(
                "LiteWeb: profil WebKit privé indisponible; utilisation d'une session éphémère"
            );
            WebsiteDataManager::builder().is_ephemeral(true).build()
        }
    }
}

pub fn sandbox_runtime_available() -> bool {
    binary_is_available(&["bwrap", "/usr/bin/bwrap", "/bin/bwrap"])
}

fn binary_is_available(candidates: &[&str]) -> bool {
    candidates.iter().any(|candidate| {
        if candidate.contains('/') {
            Path::new(candidate).is_file()
        } else {
            std::env::var_os("PATH")
                .map(|paths| std::env::split_paths(&paths).any(|dir| dir.join(candidate).is_file()))
                .unwrap_or(false)
        }
    })
}

fn prepare_private_dir(path: &std::path::Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        set_private_dir_permissions(parent)?;
    }
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "le profil WebKit ne doit pas être un lien symbolique",
            ));
        }
        Ok(metadata) if !metadata.file_type().is_dir() => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "le profil WebKit doit être un répertoire",
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    std::fs::create_dir_all(path)?;
    if std::fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "le profil WebKit ne doit pas être un lien symbolique",
        ));
    }
    set_private_dir_permissions(path)
}

#[cfg(unix)]
fn set_private_dir_permissions(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn set_private_dir_permissions(_path: &std::path::Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::energy::EnergyLevel;

    #[test]
    fn ultra_switches_are_all_off() {
        let s = engine_switches(EnergyLevel::Ultra.webview_policy());
        assert!(!s.javascript && !s.images && !s.media);
        assert!(
            s.hardware_acceleration,
            "Ultra must keep the compositor that handles trackpad SMOOTH events"
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_webkit_profile_symlink() {
        let root = std::env::temp_dir().join(format!(
            "liteweb-webkit-symlink-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let target = root.join("target");
        std::fs::create_dir_all(&target).unwrap();
        let link = root.join("webkit");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        assert!(prepare_private_dir(&link).is_err());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sandbox_probe_rejects_missing_binaries() {
        assert!(!binary_is_available(&[
            "/liteweb-does-not-exist/bwrap",
            "liteweb-does-not-exist-bwrap"
        ]));
    }

    #[cfg(unix)]
    #[test]
    fn sandbox_probe_finds_a_real_system_binary() {
        assert!(
            binary_is_available(&["/bin/sh", "/usr/bin/sh", "sh"]),
            "sandbox probe must recognize an existing executable"
        );
    }
}
