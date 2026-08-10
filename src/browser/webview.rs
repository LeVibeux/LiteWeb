use std::rc::Rc;

use glib::Cast;
use webkit2gtk::{
    CacheModel, NavigationPolicyDecision, NavigationPolicyDecisionExt, PolicyDecisionExt,
    PolicyDecisionType, ResponsePolicyDecision, ResponsePolicyDecisionExt, SettingsExt,
    URIRequestExt, URIResponseExt, WebContext, WebContextExt, WebView, WebViewExt,
};

use crate::adblock::Blocker;

pub fn create_webview(blocker: Rc<Blocker>) -> WebView {
    let context = WebContext::default().expect("failed to create web context");
    context.set_cache_model(CacheModel::DocumentViewer);

    let webview = WebView::with_context(&context);

    if let Some(settings) = webview.settings() {
        settings.set_enable_page_cache(false);
        settings.set_enable_offline_web_application_cache(false);
        settings.set_enable_hyperlink_auditing(false);
        settings.set_enable_dns_prefetching(false);
        settings.set_enable_developer_extras(false);
        settings.set_media_playback_requires_user_gesture(true);
        settings.set_javascript_can_open_windows_automatically(false);
    }

    let blocker_policy = blocker.clone();
    webview.connect_decide_policy(move |_view, decision, decision_type| {
        let uri = match decision_type {
            PolicyDecisionType::NavigationAction | PolicyDecisionType::NewWindowAction => decision
                .dynamic_cast_ref::<NavigationPolicyDecision>()
                .and_then(|d| d.request())
                .and_then(|r| r.uri()),
            PolicyDecisionType::Response => decision
                .dynamic_cast_ref::<ResponsePolicyDecision>()
                .and_then(|d| d.response())
                .and_then(|r| r.uri()),
            _ => None,
        };

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
