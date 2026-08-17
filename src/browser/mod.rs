mod reader;
mod tab;
mod tab_manager;
mod webview;

pub use reader::flatten_html;
pub use tab::{Tab, TabState};
pub use tab_manager::TabManager;
pub use webview::{create_web_context, create_webview};
