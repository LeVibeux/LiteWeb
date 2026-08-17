use std::rc::Rc;

use glib::Cast;
use webkit2gtk::{
    CacheModel, FileChooserRequestExt, HardwareAccelerationPolicy, NavigationPolicyDecision,
    NavigationPolicyDecisionExt, NotificationExt, PermissionRequestExt, PolicyDecisionExt,
    PolicyDecisionType, ProcessModel, SettingsExt, TLSErrorsPolicy, URIRequestExt,
    UserContentInjectedFrames, UserContentManager, UserContentManagerExt, UserStyleLevel,
    UserStyleSheet, WebContext, WebContextExt, WebView, WebViewExt, WebViewExtManual,
    WebsiteDataManager,
};

use crate::adblock::Blocker;
use crate::commands::is_safe_navigation_url;
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

pub fn create_webview(
    context: &WebContext,
    blocker: Rc<Blocker>,
    content: &UserContentManager,
    policy: WebViewPolicy,
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

    let blocker_policy = blocker.clone();
    webview.connect_decide_policy(move |_view, decision, decision_type| {
        if !matches!(
            decision_type,
            PolicyDecisionType::NavigationAction | PolicyDecisionType::NewWindowAction
        ) {
            return false;
        }

        let uri = decision
            .dynamic_cast_ref::<NavigationPolicyDecision>()
            .and_then(|d| d.navigation_action())
            .and_then(|action| action.request())
            .and_then(|r| r.uri());

        if let Some(uri) = uri {
            if !is_safe_navigation_url(uri.as_str()) || blocker_policy.check(uri.as_str()) {
                decision.ignore();
                return true;
            }
        }
        false
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

fn prepare_private_dir(path: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)?;
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
}
