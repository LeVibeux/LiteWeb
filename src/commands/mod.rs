mod palette;

pub(crate) use palette::{
    display_navigation_url, is_safe_navigation_url, sanitize_ui_text, should_allow_navigation,
};
pub use palette::{CommandAction, CommandPalette};
