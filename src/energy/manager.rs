use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnergyLevel {
    Normal,
    Eco,
    Aggressive,
    Ultra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebViewPolicy {
    pub javascript: bool,
    pub javascript_markup: bool,
    pub auto_load_images: bool,
    pub media: bool,
    pub webaudio: bool,
    pub webgl: bool,
    pub webrtc: bool,
    pub html5_local_storage: bool,
    pub html5_database: bool,
    pub hardware_acceleration: bool,
    pub smooth_scrolling: bool,
    pub archaic_stylesheet: bool,
    pub flatten_document: bool,
}

impl EnergyLevel {
    pub fn next(self) -> Self {
        match self {
            Self::Normal => Self::Eco,
            Self::Eco => Self::Aggressive,
            Self::Aggressive => Self::Ultra,
            Self::Ultra => Self::Normal,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Eco => "Éco",
            Self::Aggressive => "Agressif",
            Self::Ultra => "Ultra",
        }
    }

    pub fn suspend_timeout(self) -> Duration {
        match self {
            Self::Normal => Duration::from_secs(600),
            Self::Eco => Duration::from_secs(180),
            Self::Aggressive => Duration::from_secs(60),
            Self::Ultra => Duration::from_secs(15),
        }
    }

    pub fn max_active_tabs(self) -> usize {
        match self {
            Self::Normal => 20,
            Self::Eco => 10,
            Self::Aggressive => 5,
            Self::Ultra => 1,
        }
    }

    pub fn webview_policy(self) -> WebViewPolicy {
        let nuclear = matches!(self, Self::Ultra);
        WebViewPolicy {
            javascript: !nuclear,
            javascript_markup: !nuclear,
            auto_load_images: !nuclear,
            media: !nuclear,
            webaudio: !nuclear,
            webgl: !nuclear,
            webrtc: false,
            html5_local_storage: !nuclear,
            html5_database: !nuclear,
            // Keep the accelerated compositor and smooth scrolling. Two-finger
            // trackpads emit GDK_SCROLL_SMOOTH; WebKit only consumes those on
            // this path. JS/images/media remain the RAM win.
            hardware_acceleration: true,
            smooth_scrolling: true,
            archaic_stylesheet: nuclear,
            flatten_document: nuclear,
        }
    }

    pub fn block_autoplay(self) -> bool {
        !matches!(self, Self::Normal)
    }

    pub fn throttle_background_js(self) -> bool {
        !matches!(self, Self::Normal)
    }
}

pub struct EnergyManager {
    level: EnergyLevel,
}

impl EnergyManager {
    pub fn new() -> Self {
        Self {
            level: EnergyLevel::Normal,
        }
    }

    pub fn level(&self) -> EnergyLevel {
        self.level
    }

    pub fn set_level(&mut self, level: EnergyLevel) {
        self.level = level;
    }

    pub fn toggle(&mut self) -> EnergyLevel {
        self.level = self.level.next();
        self.level
    }

    pub fn should_suspend(&self, last_active: Instant, now: Instant) -> bool {
        now.duration_since(last_active) >= self.level.suspend_timeout()
    }

    pub fn tabs_to_suspend(&self, active_count: usize) -> usize {
        active_count.saturating_sub(self.level.max_active_tabs())
    }
}

impl Default for EnergyManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eco_suspends_faster_than_normal() {
        let eco = EnergyManager {
            level: EnergyLevel::Eco,
        };
        let normal = EnergyManager {
            level: EnergyLevel::Normal,
        };
        assert!(eco.level().suspend_timeout() < normal.level().suspend_timeout());
    }

    #[test]
    fn aggressive_limits_tabs() {
        let mgr = EnergyManager {
            level: EnergyLevel::Aggressive,
        };
        assert_eq!(mgr.tabs_to_suspend(8), 3);
    }

    #[test]
    fn cycle_includes_ultra() {
        assert_eq!(EnergyLevel::Aggressive.next(), EnergyLevel::Ultra);
        assert_eq!(EnergyLevel::Ultra.next(), EnergyLevel::Normal);
    }

    #[test]
    fn ultra_is_stricter_than_aggressive() {
        let ultra = EnergyLevel::Ultra;
        let agg = EnergyLevel::Aggressive;
        assert!(ultra.suspend_timeout() < agg.suspend_timeout());
        assert!(ultra.max_active_tabs() < agg.max_active_tabs());
        assert_eq!(ultra.max_active_tabs(), 1);
        assert_eq!(ultra.suspend_timeout(), Duration::from_secs(15));
    }

    #[test]
    fn ultra_policy_disables_expensive_engine_features() {
        let p = EnergyLevel::Ultra.webview_policy();
        assert!(!p.javascript);
        assert!(!p.javascript_markup);
        assert!(!p.auto_load_images);
        assert!(!p.media);
        assert!(!p.webaudio);
        assert!(!p.webgl);
        assert!(!p.webrtc);
        assert!(!p.html5_local_storage);
        assert!(!p.html5_database);
        assert!(p.archaic_stylesheet);
        assert!(p.flatten_document);
    }

    #[test]
    fn ultra_keeps_touchpad_scroll_path() {
        let p = EnergyLevel::Ultra.webview_policy();
        assert!(
            p.smooth_scrolling,
            "two-finger trackpads emit GDK_SCROLL_SMOOTH; WebKit drops them if smooth scrolling is off"
        );
        assert!(
            p.hardware_acceleration,
            "software compositing (HA Never) does not consume trackpad SMOOTH events"
        );
    }

    #[test]
    fn normal_policy_keeps_a_usable_engine() {
        let p = EnergyLevel::Normal.webview_policy();
        assert!(p.javascript);
        assert!(p.auto_load_images);
        assert!(p.hardware_acceleration);
        assert!(!p.archaic_stylesheet);
        assert!(!p.flatten_document);
    }

    #[test]
    fn ultra_label() {
        assert_eq!(EnergyLevel::Ultra.label(), "Ultra");
    }
}
