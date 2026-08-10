use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnergyLevel {
    Normal,
    Eco,
    Aggressive,
}

impl EnergyLevel {
    pub fn next(self) -> Self {
        match self {
            Self::Normal => Self::Eco,
            Self::Eco => Self::Aggressive,
            Self::Aggressive => Self::Normal,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Eco => "Éco",
            Self::Aggressive => "Agressif",
        }
    }

    pub fn suspend_timeout(self) -> Duration {
        match self {
            Self::Normal => Duration::from_secs(600),
            Self::Eco => Duration::from_secs(180),
            Self::Aggressive => Duration::from_secs(60),
        }
    }

    pub fn max_active_tabs(self) -> usize {
        match self {
            Self::Normal => 20,
            Self::Eco => 10,
            Self::Aggressive => 5,
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
}
