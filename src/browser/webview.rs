use std::rc::Rc;

use glib::Cast;
use webkit2gtk::{
    CacheModel, FileChooserRequestExt, NavigationPolicyDecision, NavigationPolicyDecisionExt,
    NotificationExt, PermissionRequestExt, PolicyDecisionExt, PolicyDecisionType, ProcessModel,
    SettingsExt, TLSErrorsPolicy, URIRequestExt, WebContext, WebContextExt, WebView, WebViewExt,
    WebsiteDataManager,
};

use crate::adblock::Blocker;
use crate::commands::is_safe_navigation_url;

pub fn create_webview(context: &WebContext, blocker: Rc<Blocker>) -> WebView {
    let webview = WebView::with_context(context);

    if let Some(settings) = webview.settings() {
        settings.set_enable_javascript(true);
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
        settings.set_enable_webrtc(false);
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
