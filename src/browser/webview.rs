use std::rc::Rc;

use glib::Cast;
use webkit2gtk::{
    CacheModel, NavigationPolicyDecision, NavigationPolicyDecisionExt, PolicyDecisionExt,
    PolicyDecisionType, SettingsExt, URIRequestExt, WebContext, WebContextExt, WebView,
    WebViewExt,
};

use crate::adblock::Blocker;

pub fn create_webview(context: &WebContext, blocker: Rc<Blocker>) -> WebView {
    let webview = WebView::with_context(context);

    if let Some(settings) = webview.settings() {
        settings.set_enable_javascript(true);
        settings.set_enable_page_cache(true);
        settings.set_enable_offline_web_application_cache(false);
        settings.set_enable_dns_prefetching(true);
        settings.set_enable_developer_extras(false);
        settings.set_media_playback_requires_user_gesture(true);
        settings.set_javascript_can_open_windows_automatically(false);
    }

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
            .and_then(|d| d.request())
            .and_then(|r| r.uri());

        if let Some(uri) = uri {
            if blocker_policy.check(uri.as_str()) {
                decision.ignore();
                return true;
            }
        }
        false
    });

    webview
}

pub fn create_web_context() -> WebContext {
    let context = WebContext::default().expect("failed to create web context");
    context.set_cache_model(CacheModel::WebBrowser);
    context
}
