use gtk::Label;
use std::time::Instant;
use webkit2gtk::WebView;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabState {
    Active,
    Background,
    Suspended,
    Error,
}

pub struct Tab {
    pub id: usize,
    pub url: String,
    pub title: String,
    pub state: TabState,
    pub modified: bool,
    pub scroll_x: i32,
    pub scroll_y: i32,
    pub last_active: Instant,
    pub webview: Option<WebView>,
    pub tab_label: Option<Label>,
}

impl Tab {
    pub fn new(id: usize, url: impl Into<String>) -> Self {
        Self {
            id,
            url: url.into(),
            title: String::from("Nouvel onglet"),
            state: TabState::Background,
            modified: false,
            scroll_x: 0,
            scroll_y: 0,
            last_active: Instant::now(),
            webview: None,
            tab_label: None,
        }
    }

    pub fn touch(&mut self) {
        self.last_active = Instant::now();
    }

    pub fn is_suspended(&self) -> bool {
        self.state == TabState::Suspended
    }

    pub fn suspend(&mut self) {
        self.webview = None;
        self.state = TabState::Suspended;
    }

    pub fn wake(&mut self) {
        self.state = TabState::Background;
        self.last_active = Instant::now();
    }
}
