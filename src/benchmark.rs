use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Fixed, public pages used by the consumption benchmark. Keeping this list in
/// the binary makes every invocation of the script use the same workload.
pub const BENCHMARK_URLS: [&str; 10] = [
    "https://www.google.com/",
    "https://fr.wikipedia.org/wiki/Navigateur_web",
    "https://www.rust-lang.org/fr/",
    "https://developer.mozilla.org/fr/",
    "https://github.com/",
    "https://www.mozilla.org/fr/",
    "https://ubuntu.com/",
    "https://stackoverflow.com/",
    "https://news.ycombinator.com/",
    "https://www.reddit.com/",
];

pub const WARMUP_SECS: u64 = 30;
pub const IDLE_MEASUREMENT_SECS: u64 = 120;
pub const POST_SUSPENSION_SECS: u64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchmarkScenario {
    Idle,
    Normal,
    Aggressive,
    Ultra,
}

impl BenchmarkScenario {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "idle" => Some(Self::Idle),
            "normal" => Some(Self::Normal),
            "aggressive" => Some(Self::Aggressive),
            "ultra" => Some(Self::Ultra),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Normal => "normal",
            Self::Aggressive => "aggressive",
            Self::Ultra => "ultra",
        }
    }

    pub fn expected_suspended_tabs(self) -> usize {
        match self {
            Self::Idle => 0,
            Self::Normal | Self::Aggressive | Self::Ultra => BENCHMARK_URLS.len(),
        }
    }
}

impl fmt::Display for BenchmarkScenario {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    pub scenario: BenchmarkScenario,
    pub state_file: PathBuf,
}

impl BenchmarkConfig {
    /// Parses the private interface used by `scripts/benchmark_consumption.sh`.
    /// Normal interactive startup keeps its existing argument-free behavior.
    pub fn from_args(args: &[String]) -> Result<Option<Self>, String> {
        let Some(position) = args.iter().position(|arg| arg == "--benchmark") else {
            return Ok(None);
        };

        let scenario_value = args
            .get(position + 1)
            .ok_or_else(|| "--benchmark requires idle, normal, aggressive, or ultra".to_string())?;
        let scenario = BenchmarkScenario::parse(scenario_value).ok_or_else(|| {
            format!(
                "unknown benchmark scenario '{scenario_value}'; expected idle, normal, aggressive, or ultra"
            )
        })?;

        let state_position = args
            .iter()
            .position(|arg| arg == "--benchmark-state")
            .ok_or_else(|| "--benchmark requires --benchmark-state <path>".to_string())?;
        let state_file = args
            .get(state_position + 1)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "--benchmark-state requires a file path".to_string())?;

        Ok(Some(Self {
            scenario,
            state_file: PathBuf::from(state_file),
        }))
    }

    pub fn initial_urls(&self) -> Vec<String> {
        match self.scenario {
            BenchmarkScenario::Idle => vec![BENCHMARK_URLS[0].to_string()],
            BenchmarkScenario::Normal
            | BenchmarkScenario::Aggressive
            | BenchmarkScenario::Ultra => {
                let mut urls = BENCHMARK_URLS
                    .iter()
                    .map(|url| (*url).to_string())
                    .collect::<Vec<_>>();
                // The active blank page is intentionally not part of the workload. It allows
                // all ten measured pages to become inactive and therefore suspended.
                urls.push("about:blank".to_string());
                urls
            }
        }
    }
}

/// A small CSV event stream consumed by the shell benchmark. It deliberately
/// avoids a JSON parser dependency in the collection path.
pub struct BenchmarkReporter {
    config: BenchmarkConfig,
    started_at: Instant,
}

impl BenchmarkReporter {
    pub fn new(config: BenchmarkConfig) -> Self {
        if let Some(parent) = config.state_file.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                eprintln!("LiteWeb benchmark: cannot create state directory: {error}");
            }
        }
        if let Err(error) = fs::write(
            &config.state_file,
            "event,elapsed_ms,wall_time_ms,suspended_tabs\n",
        ) {
            eprintln!("LiteWeb benchmark: cannot initialize state file: {error}");
        }

        let reporter = Self {
            config,
            started_at: Instant::now(),
        };
        reporter.event("run_started", 0);
        reporter
    }

    pub fn scenario(&self) -> BenchmarkScenario {
        self.config.scenario
    }

    pub fn elapsed(&self) -> std::time::Duration {
        self.started_at.elapsed()
    }

    pub fn event(&self, name: &str, suspended_tabs: usize) {
        let elapsed_ms = self.started_at.elapsed().as_millis();
        let wall_time_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default();
        let line = format!("{name},{elapsed_ms},{wall_time_ms},{suspended_tabs}\n");

        match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.config.state_file)
        {
            Ok(mut file) => {
                if let Err(error) = file.write_all(line.as_bytes()) {
                    eprintln!("LiteWeb benchmark: cannot write state event: {error}");
                }
            }
            Err(error) => eprintln!("LiteWeb benchmark: cannot open state file: {error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_each_supported_scenario() {
        assert_eq!(
            BenchmarkScenario::parse("idle"),
            Some(BenchmarkScenario::Idle)
        );
        assert_eq!(
            BenchmarkScenario::parse("normal"),
            Some(BenchmarkScenario::Normal)
        );
        assert_eq!(
            BenchmarkScenario::parse("aggressive"),
            Some(BenchmarkScenario::Aggressive)
        );
        assert_eq!(
            BenchmarkScenario::parse("ultra"),
            Some(BenchmarkScenario::Ultra)
        );
        assert_eq!(BenchmarkScenario::parse("eco"), None);
    }

    #[test]
    fn ultra_workload_matches_aggressive() {
        let config = BenchmarkConfig {
            scenario: BenchmarkScenario::Ultra,
            state_file: PathBuf::from("/tmp/liteweb-benchmark-state.tsv"),
        };
        let urls = config.initial_urls();
        assert_eq!(urls.len(), BENCHMARK_URLS.len() + 1);
        assert_eq!(urls.last().map(String::as_str), Some("about:blank"));
        assert_eq!(
            config.scenario.expected_suspended_tabs(),
            BENCHMARK_URLS.len()
        );
    }

    #[test]
    fn benchmark_arguments_require_a_state_path() {
        let args = vec!["liteweb".into(), "--benchmark".into(), "idle".into()];
        assert!(BenchmarkConfig::from_args(&args).is_err());
    }

    #[test]
    fn normal_workload_has_ten_measured_tabs_and_a_sentinel() {
        let config = BenchmarkConfig {
            scenario: BenchmarkScenario::Normal,
            state_file: PathBuf::from("/tmp/liteweb-benchmark-state.tsv"),
        };
        let urls = config.initial_urls();
        assert_eq!(urls.len(), BENCHMARK_URLS.len() + 1);
        assert_eq!(urls.last().map(String::as_str), Some("about:blank"));
        assert_eq!(
            config.scenario.expected_suspended_tabs(),
            BENCHMARK_URLS.len()
        );
    }
}
