//! User configuration: a TOML file with built-in defaults behind it.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::core::Severity;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub general: General,
    pub ui: Ui,
    pub sampling: Sampling,
    pub thresholds: Thresholds,
    pub alerts: Alerts,
    pub actions: Actions,
    pub detection: Detection,
    pub history: History,
    pub daemon: Daemon,
    pub pressure: Pressure,
    pub policies: Policies,
    pub critical_processes: CriticalProcesses,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct General {
    pub profile: Profile,
    pub low_resource_mode: bool,
    #[serde(deserialize_with = "de_duration")]
    pub interval: Duration,
    #[serde(deserialize_with = "de_duration")]
    pub history_duration: Duration,
    pub persist_history: bool,
    /// Where history is stored. `None` means the platform default.
    pub data_dir: Option<PathBuf>,
}

impl Default for General {
    fn default() -> Self {
        Self {
            profile: Profile::Default,
            low_resource_mode: false,
            interval: Duration::from_secs(1),
            history_duration: Duration::from_secs(600),
            persist_history: false,
            data_dir: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Profile {
    #[default]
    Default,
    LowResource,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ui {
    pub theme: String,
    pub show_network: bool,
    pub show_temperature: bool,
}

impl Default for Ui {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            show_network: true,
            show_temperature: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Sampling {
    pub cpu: bool,
    pub memory: bool,
    pub disk: bool,
    pub network: bool,
    pub temperature: bool,
    pub process_io: bool,
    pub process_network: bool,
}

impl Default for Sampling {
    fn default() -> Self {
        Self {
            cpu: true,
            memory: true,
            disk: true,
            network: true,
            temperature: true,
            process_io: true,
            process_network: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Thresholds {
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub temperature_celsius: f64,
    pub disk_usage_percent: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            cpu_percent: 90.0,
            memory_percent: 90.0,
            temperature_celsius: 85.0,
            disk_usage_percent: 90.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Alerts {
    pub enabled: bool,
    /// `None` means no alert log.
    pub log_file: Option<PathBuf>,
    /// The rules that watch the metrics and raise alerts.
    pub rules: Vec<AlertRule>,
}

impl Default for Alerts {
    fn default() -> Self {
        Self {
            enabled: true,
            log_file: None,
            rules: Vec::new(),
        }
    }
}

/// A rule that watches one metric and raises an alert when it stays past its
/// threshold for long enough.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AlertRule {
    pub name: String,
    /// The metric to watch, for example `cpu_percent`.
    pub metric: String,
    pub condition: Condition,
    /// How a spike is measured, for the `spike` condition.
    pub spike: Option<SpikeOptions>,
    pub threshold: f64,
    /// How long the condition must hold before the alert is raised.
    #[serde(deserialize_with = "de_duration")]
    pub duration: Duration,
    pub severity: Severity,
    pub message: String,
    pub enabled: bool,
    /// How long the rule stays quiet after it has fired.
    #[serde(deserialize_with = "de_duration")]
    pub cooldown: Duration,
}

impl Default for AlertRule {
    fn default() -> Self {
        Self {
            name: String::new(),
            metric: String::new(),
            condition: Condition::Above,
            spike: None,
            threshold: 0.0,
            duration: Duration::from_secs(0),
            severity: Severity::Warning,
            message: String::new(),
            enabled: true,
            cooldown: Duration::from_secs(60),
        }
    }
}

/// How the baseline of a spike is worked out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Baseline {
    #[default]
    MovingAverage,
    Ewma,
}

/// How much of the recent past a spike is compared against.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SpikeOptions {
    pub baseline: Baseline,
    #[serde(deserialize_with = "de_duration")]
    pub window: Duration,
    /// How many times the baseline the value has to be.
    pub multiple: f64,
    /// The value has to be at least this, so a small fluctuation is not a spike.
    pub minimum: f64,
}

impl Default for SpikeOptions {
    fn default() -> Self {
        Self {
            baseline: Baseline::default(),
            window: Duration::from_secs(30),
            multiple: 2.0,
            minimum: 0.0,
        }
    }
}

/// Which side of the threshold counts as a problem.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Condition {
    #[default]
    Above,
    Below,
    Spike,
}

impl Condition {
    /// Whether the value is a problem.
    ///
    /// The `spike` condition is worked out against a baseline, so it is not
    /// decided here.
    pub fn holds(&self, value: f64, threshold: f64) -> bool {
        match self {
            Condition::Above => value > threshold,
            Condition::Below => value < threshold,
            Condition::Spike => false,
        }
    }
}

/// Where the daemon writes what it is doing.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Daemon {
    /// Where the daemon log is written. `None` means standard output.
    pub log_file: Option<PathBuf>,
}

/// How much history is kept on disk.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct History {
    /// How much space the history file may take.
    pub max_bytes: u64,
}

impl Default for History {
    fn default() -> Self {
        Self {
            max_bytes: 10 * 1024 * 1024,
        }
    }
}

/// The policies that decide what to do about a problem.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Policies {
    /// When true the actions are only listed, never taken.
    pub dry_run: bool,
    pub rules: Vec<Policy>,
}

/// A rule that matches a condition and proposes an action.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    pub name: String,
    /// A policy does nothing unless it is turned on.
    pub enabled: bool,
    /// The metric to watch, for example `process_cpu_percent`.
    pub metric: String,
    pub condition: Condition,
    pub threshold: f64,
    /// How long the condition must hold before the policy acts.
    #[serde(deserialize_with = "de_duration")]
    pub duration: Duration,
    pub action: PolicyAction,
    /// How long the policy stays quiet after it has acted.
    #[serde(deserialize_with = "de_duration")]
    pub cooldown: Duration,
    /// Whether the user has to agree before the action is taken.
    pub require_confirmation: bool,
    /// Processes this policy never touches.
    pub exclude: Vec<String>,
    /// Whether the policy may act on a protected process.
    pub allow_protected: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            name: String::new(),
            enabled: false,
            metric: String::new(),
            condition: Condition::Above,
            threshold: 0.0,
            duration: Duration::from_secs(0),
            action: PolicyAction::Kill,
            cooldown: Duration::from_secs(300),
            require_confirmation: false,
            exclude: Vec::new(),
            allow_protected: false,
        }
    }
}

/// What a policy can do.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PolicyAction {
    #[default]
    Kill,
    Suspend,
    Resume,
    Renice,
    Throttle,
}

impl PolicyAction {
    /// The name of the action, as it is shown.
    pub fn label(&self) -> &'static str {
        match self {
            PolicyAction::Kill => "kill",
            PolicyAction::Suspend => "suspend",
            PolicyAction::Resume => "resume",
            PolicyAction::Renice => "renice",
            PolicyAction::Throttle => "throttle",
        }
    }
}

/// How much each resource counts towards the pressure score.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Pressure {
    pub cpu_weight: f64,
    pub memory_weight: f64,
    pub io_weight: f64,
    pub network_weight: f64,
    pub file_descriptor_weight: f64,
    pub restart_weight: f64,
}

impl Default for Pressure {
    fn default() -> Self {
        Self {
            cpu_weight: 1.0,
            memory_weight: 1.0,
            io_weight: 1.0,
            network_weight: 1.0,
            file_descriptor_weight: 1.0,
            restart_weight: 1.0,
        }
    }
}

impl Pressure {
    /// The weights, as the scorer wants them.
    pub fn weights(&self) -> crate::pressure::Weights {
        crate::pressure::Weights {
            cpu: self.cpu_weight,
            memory: self.memory_weight,
            io: self.io_weight,
            network: self.network_weight,
            file_descriptors: self.file_descriptor_weight,
            restarts: self.restart_weight,
        }
    }
}

/// How the tool spots what has gone wrong.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Detection {
    /// Whether to look for OOM evidence in the kernel log.
    pub oom_evidence: bool,
    /// How many restarts in the window count as a restart loop.
    pub restart_count: u32,
    /// The window the restarts are counted in.
    #[serde(deserialize_with = "de_duration")]
    pub restart_window: Duration,
}

impl Default for Detection {
    fn default() -> Self {
        Self {
            oom_evidence: true,
            restart_count: 5,
            restart_window: Duration::from_secs(60),
        }
    }
}

/// What happens when the tool acts on a process.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Actions {
    /// Where the action log is written. `None` means no log.
    pub log_file: Option<PathBuf>,
    /// Whether to ask before acting.
    pub confirm: bool,
}

impl Default for Actions {
    fn default() -> Self {
        Self {
            log_file: None,
            confirm: true,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CriticalProcesses {
    pub names: Vec<String>,
}

impl Config {
    /// Parse configuration text. Missing keys and missing sections fall back to defaults.
    pub fn from_toml(text: &str) -> Result<Config, ConfigError> {
        let mut config: Config =
            toml::from_str(text).map_err(|e| ConfigError::parse(describe_error(text, &e)))?;
        config.general.data_dir = expand_home(config.general.data_dir);
        config.alerts.log_file = expand_home(config.alerts.log_file);
        config.actions.log_file = expand_home(config.actions.log_file);
        config.daemon.log_file = expand_home(config.daemon.log_file);
        config.apply_profile();
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        for (key, value) in [
            ("thresholds.cpu_percent", self.thresholds.cpu_percent),
            ("thresholds.memory_percent", self.thresholds.memory_percent),
            (
                "thresholds.disk_usage_percent",
                self.thresholds.disk_usage_percent,
            ),
        ] {
            if !(0.0..=100.0).contains(&value) {
                return Err(ConfigError::parse(format!(
                    "{key}: must be between 0 and 100, got {value}"
                )));
            }
        }

        let temperature = self.thresholds.temperature_celsius;
        if !temperature.is_finite() || temperature <= 0.0 {
            return Err(ConfigError::parse(format!(
                "thresholds.temperature_celsius: must be greater than 0, got {temperature}"
            )));
        }
        Ok(())
    }

    /// Load a config file. A missing file is not an error: defaults apply.
    pub fn from_path(path: &Path) -> Result<Config, ConfigError> {
        match fs::read_to_string(path) {
            Ok(text) => Config::from_toml(&text)
                .map_err(|e| ConfigError::in_file(path.to_path_buf(), e.detail)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(ConfigError::in_file(path.to_path_buf(), e.to_string())),
        }
    }

    /// Local `sentinel.toml` wins; otherwise the user config; otherwise defaults.
    pub fn load() -> Result<Config, ConfigError> {
        load_from(Path::new("sentinel.toml"), user_config_path().as_deref())
    }

    fn apply_profile(&mut self) {
        if self.general.profile != Profile::LowResource && !self.general.low_resource_mode {
            return;
        }
        self.general.interval = self.general.interval.max(Duration::from_secs(2));
        self.general.history_duration = self.general.history_duration.min(Duration::from_secs(60));
        self.general.persist_history = false;
        self.sampling.process_io = false;
        self.sampling.process_network = false;
    }
}

fn load_from(local: &Path, user: Option<&Path>) -> Result<Config, ConfigError> {
    if local.exists() {
        return Config::from_path(local);
    }
    if let Some(user) = user
        && user.exists()
    {
        return Config::from_path(user);
    }
    Ok(Config::default())
}

/// `~` and `~/x` appear in the docs; a literal `~` directory is never intended.
fn expand_home(path: Option<PathBuf>) -> Option<PathBuf> {
    let path = path?;
    let text = path.to_string_lossy().into_owned();
    let rest = match text.as_str() {
        "~" => "",
        other => match other.strip_prefix("~/") {
            Some(rest) => rest,
            None => return Some(path),
        },
    };
    match std::env::var_os("HOME") {
        Some(home) => Some(PathBuf::from(home).join(rest)),
        None => Some(path),
    }
}

fn user_config_path() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
    };
    Some(base.join("sentinel").join("config.toml"))
}

fn de_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let text = String::deserialize(deserializer)?;
    parse_duration(&text).map_err(serde::de::Error::custom)
}

const DURATION_UNITS: [(&str, f64); 4] = [
    ("ms", 1.0),
    ("s", 1_000.0),
    ("m", 60_000.0),
    ("h", 3_600_000.0),
];

fn unit_list() -> String {
    DURATION_UNITS
        .iter()
        .map(|(unit, _)| *unit)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Parse a duration such as `500ms`, `2s`, `10m` or `1h`.
pub fn parse_duration(text: &str) -> Result<Duration, String> {
    let text = text.trim();
    let split = text
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .ok_or_else(|| {
            let units = unit_list();
            format!("`{text}` is missing a unit ({units})")
        })?;
    let (number, unit) = text.split_at(split);
    if number.is_empty() {
        return Err(format!("`{text}` is missing a number"));
    }
    let value: f64 = number
        .parse()
        .map_err(|_| format!("`{text}` has an invalid number"))?;
    let millis = DURATION_UNITS
        .iter()
        .find(|(suffix, _)| *suffix == unit)
        .map(|(_, scale)| value * scale)
        .ok_or_else(|| {
            let units = unit_list();
            format!("`{text}` uses an unknown unit `{unit}` (expected {units})")
        })?;
    if !millis.is_finite() || millis <= 0.0 {
        return Err(format!(
            "`{text}` must be a finite duration greater than zero"
        ));
    }
    Duration::try_from_secs_f64(millis / 1_000.0)
        .map_err(|_| format!("`{text}` is too large to represent"))
}

/// Prefix the parser's message with the dotted key path, which TOML errors omit.
fn describe_error(text: &str, error: &toml::de::Error) -> String {
    let Some(span) = error.span() else {
        return error.message().to_string();
    };
    let line = text
        .get(..span.start)
        .map_or(1, |upto| upto.lines().count() + 1);
    match key_path_at(text, span.start) {
        Some(path) => format!("{path} (line {line}): {}", error.message()),
        None => format!("line {line}: {}", error.message()),
    }
}

/// shortcut: inline tables and multi-line arrays report the enclosing key rather
/// than the leaf; upgrade if configs start using them.
fn key_path_at(text: &str, byte: usize) -> Option<String> {
    let upto = text.get(..byte)?;
    let line_start = upto.rfind('\n').map_or(0, |i| i + 1);
    let line = &text[line_start..];
    let key = line.split_once('=')?.0.trim();
    if key.is_empty() || key.starts_with('[') {
        return None;
    }
    let table = upto[..line_start]
        .lines()
        .rev()
        .find_map(|line| {
            let line = line.trim();
            line.strip_prefix('[')?.strip_suffix(']')
        })
        .map_or_else(String::new, |table| format!("{table}."));
    Some(format!("{table}{key}"))
}

#[derive(Debug)]
pub struct ConfigError {
    pub path: Option<PathBuf>,
    pub detail: String,
}

impl ConfigError {
    fn parse(detail: String) -> Self {
        Self { path: None, detail }
    }

    fn in_file(path: PathBuf, detail: String) -> Self {
        Self {
            path: Some(path),
            detail,
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.path {
            Some(path) => write!(f, "{}: {}", path.display(), self.detail),
            None => write!(f, "invalid config: {}", self.detail),
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    #[test]
    fn empty_config_gives_defaults() {
        let c = Config::from_toml("").unwrap();

        assert_eq!(c.general.profile, Profile::Default);
        assert_eq!(c.general.interval, Duration::from_secs(1));
        assert_eq!(c.general.history_duration, Duration::from_secs(600));
        assert!(!c.general.persist_history);
        assert_eq!(c.general.data_dir, None);

        assert!(c.sampling.cpu);
        assert!(c.sampling.memory);
        assert!(c.sampling.disk);
        assert!(c.sampling.network);
        assert!(c.sampling.temperature);
        assert!(c.sampling.process_io);
        assert!(!c.sampling.process_network);

        assert_eq!(c.thresholds.cpu_percent, 90.0);
        assert_eq!(c.thresholds.memory_percent, 90.0);
        assert_eq!(c.thresholds.temperature_celsius, 85.0);
        assert_eq!(c.thresholds.disk_usage_percent, 90.0);

        assert!(c.alerts.enabled);
        assert_eq!(c.alerts.log_file, None);
        assert!(c.critical_processes.names.is_empty());
    }

    #[test]
    fn every_section_is_read() {
        let c = Config::from_path(&fixture("config-full.toml")).unwrap();

        assert_eq!(c.general.interval, Duration::from_millis(250));
        assert_eq!(c.general.history_duration, Duration::from_secs(300));
        assert!(c.general.persist_history);
        assert_eq!(
            c.general.data_dir.as_deref(),
            Some(Path::new("/var/lib/sentinel"))
        );

        assert!(!c.sampling.network);
        assert!(c.sampling.temperature);

        assert_eq!(c.thresholds.cpu_percent, 75.5);
        assert_eq!(c.thresholds.disk_usage_percent, 85.0);

        assert!(!c.alerts.enabled);
        assert_eq!(
            c.alerts.log_file.as_deref(),
            Some(Path::new("/tmp/sentinel-alerts.log"))
        );

        assert_eq!(c.critical_processes.names, ["sshd", "systemd"]);
    }

    #[test]
    fn low_resource_profile_shrinks_the_footprint() {
        let c = Config::from_path(&fixture("config-low-resource.toml")).unwrap();

        assert_eq!(c.general.profile, Profile::LowResource);
        assert_eq!(c.general.interval, Duration::from_secs(2));
        assert_eq!(c.general.history_duration, Duration::from_secs(60));
        assert!(!c.general.persist_history);
        assert!(!c.sampling.process_network);
    }

    #[test]
    fn low_resource_profile_keeps_a_longer_user_interval() {
        let c =
            Config::from_toml("[general]\nprofile = \"low-resource\"\ninterval = \"5s\"").unwrap();

        assert_eq!(c.general.interval, Duration::from_secs(5));
    }

    #[test]
    fn invalid_duration_names_the_offending_key() {
        let err = Config::from_path(&fixture("config-invalid.toml")).unwrap_err();
        let message = err.to_string();

        assert!(message.contains("general.interval"), "{message}");
        assert!(message.contains("config-invalid.toml"), "{message}");
    }

    #[test]
    fn unknown_profile_names_the_offending_key() {
        let err = Config::from_toml("[general]\nprofile = \"turbo\"").unwrap_err();

        assert!(err.to_string().contains("general.profile"), "{err}");
    }

    #[test]
    fn missing_file_gives_defaults() {
        let c = Config::from_path(Path::new("/nonexistent/sentinel.toml")).unwrap();

        assert_eq!(c.general.interval, Duration::from_secs(1));
    }

    #[test]
    fn unknown_key_fails_and_names_it() {
        let err = Config::from_toml("[general]\nintervall = \"5s\"").unwrap_err();

        assert!(err.to_string().contains("intervall"), "{err}");
    }

    #[test]
    fn absurd_duration_is_an_error_not_a_panic() {
        let huge = format!("[general]\ninterval = \"{}s\"", "9".repeat(400));

        assert!(Config::from_toml(&huge).is_err());
    }

    #[test]
    fn ui_section_is_read() {
        let c = Config::from_toml("[ui]\ntheme = \"light\"\nshow_network = false").unwrap();

        assert_eq!(c.ui.theme, "light");
        assert!(!c.ui.show_network);
        assert!(c.ui.show_temperature);
    }

    #[test]
    fn low_resource_mode_flag_enables_the_profile() {
        let c = Config::from_toml("[general]\nlow_resource_mode = true").unwrap();

        assert_eq!(c.general.interval, Duration::from_secs(2));
        assert!(!c.sampling.process_io);
    }

    #[test]
    fn local_config_wins_over_user_config() {
        let local = fixture("config-full.toml");
        let user = fixture("config-low-resource.toml");

        let c = load_from(&local, Some(&user)).unwrap();

        assert_eq!(c.general.interval, Duration::from_millis(250));
    }

    #[test]
    fn user_config_is_used_when_there_is_no_local_file() {
        let missing = Path::new("/nonexistent/sentinel.toml");
        let user = fixture("config-low-resource.toml");

        let c = load_from(missing, Some(&user)).unwrap();

        assert_eq!(c.general.profile, Profile::LowResource);
    }

    #[test]
    fn defaults_apply_when_no_config_exists() {
        let missing = Path::new("/nonexistent/sentinel.toml");

        let c = load_from(missing, Some(missing)).unwrap();

        assert_eq!(c.general.interval, Duration::from_secs(1));
    }

    #[test]
    fn the_specification_example_parses() {
        let c = Config::from_path(&fixture("config-spec-example.toml")).unwrap();

        assert_eq!(c.general.interval, Duration::from_secs(1));
        assert_eq!(c.ui.theme, "dark");
        assert!(c.general.persist_history);
        assert_eq!(c.critical_processes.names, ["systemd", "sshd", "init"]);
    }

    #[test]
    fn alert_rules_are_read() {
        let c = Config::from_path(&fixture("config-alert-rules.toml")).unwrap();

        assert_eq!(c.alerts.rules.len(), 2);
        let rule = &c.alerts.rules[0];
        assert_eq!(rule.name, "high-cpu");
        assert_eq!(rule.metric, "cpu_percent");
        assert_eq!(rule.condition, Condition::Above);
        assert_eq!(rule.threshold, 90.0);
        assert_eq!(rule.duration, Duration::from_secs(5));
        assert_eq!(rule.severity, Severity::Warning);
        assert!(rule.enabled);

        assert_eq!(c.alerts.rules[1].condition, Condition::Below);
        assert!(!c.alerts.rules[1].enabled);
    }

    #[test]
    fn policies_are_read() {
        let c = Config::from_toml(
            "[policies]\ndry_run = true\n\n[[policies.rules]]\nname = \"kill-hog\"\nenabled = true\nmetric = \"process_cpu_percent\"\ncondition = \"above\"\nthreshold = 90.0\nduration = \"30s\"\naction = \"kill\"\ncooldown = \"10m\"\nrequire_confirmation = true\nexclude = [\"sshd\"]\n",
        )
        .unwrap();

        assert!(c.policies.dry_run);
        assert_eq!(c.policies.rules.len(), 1);
        let rule = &c.policies.rules[0];
        assert_eq!(rule.name, "kill-hog");
        assert!(rule.enabled);
        assert_eq!(rule.metric, "process_cpu_percent");
        assert_eq!(rule.threshold, 90.0);
        assert_eq!(rule.duration, Duration::from_secs(30));
        assert_eq!(rule.action, PolicyAction::Kill);
        assert_eq!(rule.cooldown, Duration::from_secs(600));
        assert!(rule.require_confirmation);
        assert_eq!(rule.exclude, ["sshd"]);
    }

    #[test]
    fn a_policy_is_off_until_it_is_turned_on() {
        let c = Config::from_toml(
            "[[policies.rules]]\nname = \"kill-hog\"\nmetric = \"process_cpu_percent\"\n",
        )
        .unwrap();

        assert!(!c.policies.rules[0].enabled);
        assert!(!c.policies.dry_run);
    }

    #[test]
    fn the_pressure_weights_are_read() {
        let c = Config::from_toml(
            "[pressure]\ncpu_weight = 2.0\nmemory_weight = 0.5\nrestart_weight = 3.0\n",
        )
        .unwrap();

        let weights = c.pressure.weights();
        assert_eq!(weights.cpu, 2.0);
        assert_eq!(weights.memory, 0.5);
        assert_eq!(weights.restarts, 3.0);
        assert_eq!(weights.io, 1.0);
    }

    #[test]
    fn a_spike_rule_is_read() {
        let c = Config::from_path(&fixture("config-spike-rule.toml")).unwrap();

        let rule = &c.alerts.rules[0];
        assert_eq!(rule.condition, Condition::Spike);
        let spike = rule.spike.as_ref().unwrap();
        assert_eq!(spike.baseline, Baseline::Ewma);
        assert_eq!(spike.window, Duration::from_secs(60));
        assert_eq!(spike.multiple, 5.0);
        assert_eq!(spike.minimum, 50.0);
    }

    #[test]
    fn an_alert_rule_without_a_duration_is_an_error() {
        let err = Config::from_toml(
            "[[alerts.rules]]\nname = \"x\"\nmetric = \"cpu_percent\"\nduration = \"soon\"\n",
        )
        .unwrap_err();

        assert!(err.to_string().contains("duration"), "{err}");
    }

    #[test]
    fn unknown_key_under_critical_processes_fails() {
        let err = Config::from_toml("[critical_processes]\nnamess = [\"sshd\"]").unwrap_err();

        assert!(err.to_string().contains("namess"), "{err}");
    }

    #[test]
    fn tilde_paths_expand_to_home() {
        if std::env::var_os("HOME").is_none() {
            return;
        }
        let c = Config::from_toml(
            "[general]\ndata_dir = \"~/sentinel-data\"\n[alerts]\nlog_file = \"~/alerts.log\"",
        )
        .unwrap();

        let dir = c.general.data_dir.unwrap();
        let log = c.alerts.log_file.unwrap();
        assert!(!dir.to_string_lossy().starts_with('~'), "{dir:?}");
        assert!(dir.ends_with("sentinel-data"), "{dir:?}");
        assert!(!log.to_string_lossy().starts_with('~'), "{log:?}");
    }

    #[test]
    fn impossible_thresholds_are_rejected() {
        for (key, value) in [
            ("cpu_percent", "-1"),
            ("cpu_percent", "150"),
            ("disk_usage_percent", "101"),
        ] {
            let text = format!("[thresholds]\n{key} = {value}");
            let err = Config::from_toml(&text).unwrap_err();
            assert!(err.to_string().contains("thresholds."), "{err}");
        }
    }

    #[test]
    fn non_positive_temperature_threshold_is_rejected() {
        let err = Config::from_toml("[thresholds]\ntemperature_celsius = 0").unwrap_err();

        assert!(err.to_string().contains("temperature_celsius"), "{err}");
    }
}
