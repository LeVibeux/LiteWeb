mod reader;
mod tab;
mod tab_manager;
mod webview;

pub use reader::flatten_html;
pub use tab::{Tab, TabState};
pub use tab_manager::TabManager;
pub use webview::{
    apply_webview_policy, create_user_content_manager, create_web_context, create_webview,
    sandbox_runtime_available, set_archaic_stylesheet, NewTabHandler,
};
