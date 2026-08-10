use crate::browser::tab::{Tab, TabState};
use std::time::Instant;

pub struct TabManager {
    tabs: Vec<Tab>,
    active: usize,
    next_id: usize,
}

impl TabManager {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
            next_id: 1,
        }
    }

    pub fn create_tab(&mut self, url: impl Into<String>) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let tab = Tab::new(id, url);
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
        self.active
    }

    pub fn close_tab(&mut self, index: usize) -> bool {
        if self.tabs.is_empty() {
            return false;
        }
        if index >= self.tabs.len() {
            return false;
        }
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            self.active = 0;
            return true;
        }
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        }
        true
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn set_active(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = index;
            if let Some(tab) = self.tabs.get_mut(index) {
                tab.touch();
                tab.state = TabState::Active;
            }
        }
    }

    pub fn next_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        self.active = (self.active + 1) % self.tabs.len();
        self.set_active(self.active);
    }

    pub fn prev_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        self.active = if self.active == 0 {
            self.tabs.len() - 1
        } else {
            self.active - 1
        };
        self.set_active(self.active);
    }

    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn tabs_mut(&mut self) -> &mut [Tab] {
        &mut self.tabs
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active)
    }

    pub fn suspend_tab(&mut self, index: usize) {
        if let Some(tab) = self.tabs.get_mut(index) {
            if index != self.active {
                tab.suspend();
            }
        }
    }

    pub fn suspend_all_except_active(&mut self) {
        let active = self.active;
        for (i, tab) in self.tabs.iter_mut().enumerate() {
            if i != active {
                tab.suspend();
            }
        }
    }

    pub fn inactive_indices(&self, now: Instant, timeout: std::time::Duration) -> Vec<usize> {
        self.tabs
            .iter()
            .enumerate()
            .filter(|(i, tab)| {
                *i != self.active
                    && !tab.is_suspended()
                    && now.duration_since(tab.last_active) >= timeout
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn count_active_webviews(&self) -> usize {
        self.tabs
            .iter()
            .filter(|t| t.webview.is_some() && !t.is_suspended())
            .count()
    }
}

impl Default for TabManager {
    fn default() -> Self {
        Self::new()
    }
}
