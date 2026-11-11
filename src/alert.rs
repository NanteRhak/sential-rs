//! The anomaly engine: rules that watch a metric and raise alerts.

use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::{AlertRule, Baseline, Condition};
use crate::core::Severity;
use crate::core::metrics::SystemSample;
use crate::core::process::ProcessSample;

/// What an alert is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertCategory {
    Cpu,
    Memory,
    Disk,
    Network,
    Temperature,
    Process,
    System,
}

impl AlertCategory {
    /// The category a metric belongs to.
    pub fn of(metric: &str) -> AlertCategory {
        let metric = metric.trim_start_matches("process_");
        if metric.contains("cpu") {
            AlertCategory::Cpu
        } else if metric.contains("memory") || metric.contains("swap") {
            AlertCategory::Memory
        } else if metric.contains("disk") {
            AlertCategory::Disk
        } else if metric.contains("network") {
            AlertCategory::Network
        } else if metric.contains("temperature") {
            AlertCategory::Temperature
        } else if metric.starts_with("process") {
            AlertCategory::Process
        } else {
            AlertCategory::System
        }
    }

    /// What to do about it.
    pub fn suggestion(&self) -> &'static str {
        match self {
            AlertCategory::Cpu => "look at the busiest processes",
            AlertCategory::Memory => "look for a process holding too much memory",
            AlertCategory::Disk => "look at the disk queues and the busiest processes",
            AlertCategory::Network => "look at the busiest interfaces",
            AlertCategory::Temperature => "check the cooling and the load on the machine",
            AlertCategory::Process => "inspect the process and consider throttling it",
            AlertCategory::System => "look at the system summary",
        }
    }
}

/// One raised alert.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: u64,
    pub timestamp: u64,
    pub severity: Severity,
    pub category: AlertCategory,
    pub message: String,
    pub metric: Option<String>,
    pub pid: Option<u32>,
    pub process_name: Option<String>,
    pub current_value: f64,
    pub threshold: Option<f64>,
    /// The baseline a spike was compared against.
    pub baseline: Option<f64>,
    pub suggestion: String,
}

/// Watches the metrics and raises alerts when a rule's condition holds for
/// long enough.
pub struct AlertEngine {
    rules: Vec<AlertRule>,
    /// When each rule's condition started holding.
    holding_since: HashMap<String, Instant>,
    /// When each rule last fired, so it can stay quiet for its cooldown.
    last_fired: HashMap<String, Instant>,
    /// The recent values of each rule, used to work out a baseline.
    history: HashMap<String, VecDeque<(Instant, f64)>>,
    /// The exponentially weighted moving average of each rule.
    ewma: HashMap<String, f64>,
    next_id: u64,
}

impl AlertEngine {
    pub fn new(rules: Vec<AlertRule>) -> Self {
        Self {
            rules,
            holding_since: HashMap::new(),
            last_fired: HashMap::new(),
            history: HashMap::new(),
            ewma: HashMap::new(),
            next_id: 1,
        }
    }

    /// The rules being watched.
    pub fn rules(&self) -> &[AlertRule] {
        &self.rules
    }

    /// Look at a sample and raise the alerts it calls for.
    ///
    /// A rule fires only once its condition has held for the configured
    /// duration, so one noisy sample never raises an alert.
    pub fn evaluate(
        &mut self,
        sample: &SystemSample,
        processes: &[ProcessSample],
        now: Instant,
    ) -> Vec<Alert> {
        let mut raised = Vec::new();
        let rules = self.rules.clone();
        for rule in &rules {
            if !rule.enabled {
                continue;
            }
            let Some((value, pid, process_name)) = metric_value(sample, processes, &rule.metric)
            else {
                continue;
            };
            let mut baseline = None;
            let holds = match rule.condition {
                Condition::Spike => {
                    let options = rule.spike.clone().unwrap_or_default();
                    let current = self.baseline(rule, value, now);
                    baseline = Some(current);
                    let trigger = current * options.multiple;
                    value >= trigger && value >= options.minimum && current > 0.0
                }
                other => other.holds(value, rule.threshold),
            };
            if !holds {
                self.holding_since.remove(&rule.name);
                continue;
            }
            if let Some(last) = self.last_fired.get(&rule.name)
                && now.duration_since(*last) < rule.cooldown
            {
                continue;
            }
            let since = *self.holding_since.entry(rule.name.clone()).or_insert(now);
            if now.duration_since(since) < rule.duration {
                continue;
            }
            self.last_fired.insert(rule.name.clone(), now);
            let category = AlertCategory::of(&rule.metric);
            let id = self.next_id;
            self.next_id += 1;
            raised.push(Alert {
                id,
                timestamp: crate::collect::now_millis(),
                severity: rule.severity,
                category,
                message: rule.message.clone(),
                metric: Some(rule.metric.clone()),
                pid,
                process_name,
                current_value: value,
                threshold: match rule.condition {
                    Condition::Spike => baseline.map(|current| {
                        current * rule.spike.as_ref().map(|s| s.multiple).unwrap_or(1.0)
                    }),
                    _ => Some(rule.threshold),
                },
                baseline,
                suggestion: category.suggestion().to_string(),
            });
        }
        raised
    }
}

impl AlertEngine {
    /// The baseline a value is compared against, from the recent history.
    ///
    /// The current value is folded in afterwards, so it is never part of the
    /// baseline it is compared against.
    fn baseline(&mut self, rule: &AlertRule, value: f64, now: Instant) -> f64 {
        let options = rule.spike.clone().unwrap_or_default();
        let history = self.history.entry(rule.name.clone()).or_default();
        while history
            .front()
            .is_some_and(|(at, _)| now.duration_since(*at) > options.window)
        {
            history.pop_front();
        }
        let baseline = match options.baseline {
            Baseline::MovingAverage => {
                if history.is_empty() {
                    0.0
                } else {
                    history.iter().map(|(_, value)| value).sum::<f64>() / history.len() as f64
                }
            }
            Baseline::Ewma => self.ewma.get(&rule.name).copied().unwrap_or(0.0),
        };
        if options.baseline == Baseline::Ewma {
            let samples = history.len().max(1) as f64;
            let alpha = 2.0 / (samples + 1.0);
            let previous = self.ewma.get(&rule.name).copied().unwrap_or(value);
            let average = alpha * value + (1.0 - alpha) * previous;
            self.ewma.insert(rule.name.clone(), average);
        }
        history.push_back((now, value));
        baseline
    }
}

/// Append an alert to the log file, one JSON object per line.
pub fn append_alert(path: &Path, alert: &Alert) -> Result<(), String> {
    use std::io::Write;
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let line = serde_json::to_string(alert).map_err(|error| error.to_string())?;
    writeln!(file, "{line}").map_err(|error| format!("{}: {error}", path.display()))
}

/// Read the alerts from the log file, oldest first.
///
/// A line that cannot be read is skipped: a truncated log should not hide the
/// alerts that are readable.
pub fn read_alerts(path: &Path) -> Vec<Alert> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| serde_json::from_str::<Alert>(line).ok())
        .collect()
}

/// The value of a named metric, and the process it belongs to.
///
/// Metrics starting with `process_` are measured on the busiest process, so
/// the alert can name it.
pub fn metric_value(
    sample: &SystemSample,
    processes: &[ProcessSample],
    metric: &str,
) -> Option<(f64, Option<u32>, Option<String>)> {
    if let Some(name) = metric.strip_prefix("process_") {
        let busiest = processes.iter().max_by(|a, b| {
            a.cpu_percent
                .partial_cmp(&b.cpu_percent)
                .unwrap_or(std::cmp::Ordering::Equal)
        })?;
        let value = match name {
            "cpu_percent" => busiest.cpu_percent,
            "memory_percent" => busiest.mem_percent,
            "fd_count" => busiest.fd_count as f64,
            "thread_count" => busiest.thread_count as f64,
            _ => return None,
        };
        return Some((value, Some(busiest.pid), Some(busiest.name.clone())));
    }

    let value = match metric {
        "cpu_percent" => sample.cpu.total_percent,
        "load_avg_1" => sample.cpu.load_avg_1,
        "memory_percent" => {
            if sample.memory.total_bytes == 0 {
                return None;
            }
            sample.memory.used_bytes as f64 / sample.memory.total_bytes as f64 * 100.0
        }
        "available_memory_percent" => {
            if sample.memory.total_bytes == 0 {
                return None;
            }
            sample.memory.available_bytes as f64 / sample.memory.total_bytes as f64 * 100.0
        }
        "swap_percent" => {
            if sample.memory.swap_total_bytes == 0 {
                0.0
            } else {
                sample.memory.swap_used_bytes as f64 / sample.memory.swap_total_bytes as f64 * 100.0
            }
        }
        "disk_usage_percent" => sample
            .disk
            .filesystems
            .iter()
            .map(|filesystem| {
                if filesystem.total_bytes == 0 {
                    0.0
                } else {
                    filesystem.used_bytes as f64 / filesystem.total_bytes as f64 * 100.0
                }
            })
            .fold(0.0, f64::max),
        "disk_read_bytes_per_sec" => sample.disk.read_bytes_per_sec as f64,
        "disk_write_bytes_per_sec" => sample.disk.write_bytes_per_sec as f64,
        "network_bytes_per_sec" => {
            (sample.network.rx_bytes_per_sec + sample.network.tx_bytes_per_sec) as f64
        }
        "temperature_celsius" => sample
            .sensors
            .temperatures
            .iter()
            .map(|reading| reading.celsius)
            .fold(f64::MIN, f64::max)
            .max(f64::MIN),
        _ => return None,
    };
    Some((value, None, None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::metrics::{CpuSample, MemorySample};
    use crate::core::process::{ProcessSample, ProcessState};
    use std::time::Duration;

    fn rule(name: &str, threshold: f64, duration: Duration) -> AlertRule {
        AlertRule {
            name: name.to_string(),
            metric: "cpu_percent".to_string(),
            condition: Condition::Above,
            spike: None,
            threshold,
            duration,
            severity: Severity::Warning,
            message: "cpu is high".to_string(),
            enabled: true,
            cooldown: Duration::from_secs(60),
        }
    }

    fn sample(cpu: f64) -> SystemSample {
        SystemSample {
            cpu: CpuSample {
                total_percent: cpu,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn a_rule_fires_once_the_condition_has_held_for_its_duration() {
        let mut engine = AlertEngine::new(vec![rule("cpu", 90.0, Duration::from_secs(3))]);
        let start = Instant::now();

        let early = engine.evaluate(&sample(95.0), &[], start);
        assert!(early.is_empty(), "{early:?}");

        let later = engine.evaluate(&sample(95.0), &[], start + Duration::from_secs(4));
        assert_eq!(later.len(), 1);
        assert_eq!(later[0].metric.as_deref(), Some("cpu_percent"));
        assert_eq!(later[0].current_value, 95.0);
        assert_eq!(later[0].threshold, Some(90.0));
    }

    #[test]
    fn a_rule_does_not_fire_while_the_condition_lapses() {
        let mut engine = AlertEngine::new(vec![rule("cpu", 90.0, Duration::from_secs(3))]);
        let start = Instant::now();

        engine.evaluate(&sample(95.0), &[], start);
        let _ = engine.evaluate(&sample(10.0), &[], start + Duration::from_secs(2));
        let after = engine.evaluate(&sample(95.0), &[], start + Duration::from_secs(10));

        assert!(after.is_empty(), "{after:?}");
    }

    #[test]
    fn a_rule_fires_only_once_while_it_holds() {
        let mut engine = AlertEngine::new(vec![rule("cpu", 90.0, Duration::from_secs(1))]);
        let start = Instant::now();

        let _ = engine.evaluate(&sample(95.0), &[], start);
        let first = engine.evaluate(&sample(95.0), &[], start + Duration::from_secs(2));
        let second = engine.evaluate(&sample(95.0), &[], start + Duration::from_secs(3));

        assert_eq!(first.len(), 1);
        assert!(second.is_empty(), "{second:?}");
    }

    #[test]
    fn a_rule_does_not_fire_again_inside_its_cooldown() {
        let mut rule = rule("cpu", 90.0, Duration::from_secs(1));
        rule.cooldown = Duration::from_secs(30);
        let mut engine = AlertEngine::new(vec![rule]);
        let start = Instant::now();

        let _ = engine.evaluate(&sample(95.0), &[], start);
        let first = engine.evaluate(&sample(95.0), &[], start + Duration::from_secs(2));
        assert_eq!(first.len(), 1);

        let inside = engine.evaluate(&sample(95.0), &[], start + Duration::from_secs(10));
        assert!(inside.is_empty(), "{inside:?}");

        let after = engine.evaluate(&sample(95.0), &[], start + Duration::from_secs(40));
        assert_eq!(after.len(), 1);
    }

    #[test]
    fn the_log_file_receives_the_alert() {
        let path =
            std::env::temp_dir().join(format!("sentinel-alert-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let alert = Alert {
            id: 1,
            timestamp: 1_700_000_000_000,
            severity: Severity::Warning,
            category: AlertCategory::Cpu,
            message: "cpu is high".into(),
            metric: Some("cpu_percent".into()),
            pid: None,
            process_name: None,
            current_value: 95.0,
            threshold: Some(90.0),
            baseline: None,
            suggestion: "look at the busiest processes".into(),
        };

        append_alert(&path, &alert).unwrap();
        let read = read_alerts(&path);
        let _ = std::fs::remove_file(&path);

        assert_eq!(read.len(), 1);
        assert_eq!(read[0].message, "cpu is high");
        assert_eq!(read[0].current_value, 95.0);
        assert_eq!(read[0].severity, Severity::Warning);
    }

    fn spike_rule(baseline: Baseline) -> AlertRule {
        AlertRule {
            name: "spike".to_string(),
            metric: "cpu_percent".to_string(),
            condition: Condition::Spike,
            spike: Some(crate::config::SpikeOptions {
                baseline,
                window: Duration::from_secs(30),
                multiple: 3.0,
                minimum: 50.0,
            }),
            threshold: 0.0,
            duration: Duration::from_secs(0),
            severity: Severity::Warning,
            message: "cpu spiked".to_string(),
            enabled: true,
            cooldown: Duration::from_secs(60),
        }
    }

    #[test]
    fn a_spike_is_measured_against_the_moving_average() {
        let mut engine = AlertEngine::new(vec![spike_rule(Baseline::MovingAverage)]);
        let start = Instant::now();

        // A quiet machine, then one large jump.
        for step in 0..5 {
            let _ = engine.evaluate(&sample(10.0), &[], start + Duration::from_secs(step));
        }
        let raised = engine.evaluate(&sample(90.0), &[], start + Duration::from_secs(5));

        assert_eq!(raised.len(), 1, "{raised:?}");
        assert_eq!(raised[0].current_value, 90.0);
        assert_eq!(raised[0].baseline, Some(10.0));
        assert_eq!(raised[0].threshold, Some(30.0));
    }

    #[test]
    fn a_spike_is_measured_against_the_exponential_average() {
        let mut engine = AlertEngine::new(vec![spike_rule(Baseline::Ewma)]);
        let start = Instant::now();

        for step in 0..5 {
            let _ = engine.evaluate(&sample(10.0), &[], start + Duration::from_secs(step));
        }
        let raised = engine.evaluate(&sample(90.0), &[], start + Duration::from_secs(5));

        assert_eq!(raised.len(), 1, "{raised:?}");
        let baseline = raised[0].baseline.unwrap();
        assert!(baseline < 20.0, "baseline {baseline} is not an average");
        assert!(baseline > 0.0, "baseline {baseline} is not a baseline");
    }

    #[test]
    fn a_small_fluctuation_is_not_a_spike() {
        let mut engine = AlertEngine::new(vec![spike_rule(Baseline::MovingAverage)]);
        let start = Instant::now();

        for step in 0..5 {
            let _ = engine.evaluate(&sample(10.0), &[], start + Duration::from_secs(step));
        }
        let raised = engine.evaluate(&sample(12.0), &[], start + Duration::from_secs(5));

        assert!(raised.is_empty(), "{raised:?}");
    }

    #[test]
    fn a_value_below_the_minimum_is_not_a_spike() {
        let mut engine = AlertEngine::new(vec![spike_rule(Baseline::MovingAverage)]);
        let start = Instant::now();

        for step in 0..5 {
            let _ = engine.evaluate(&sample(1.0), &[], start + Duration::from_secs(step));
        }
        let raised = engine.evaluate(&sample(40.0), &[], start + Duration::from_secs(5));

        assert!(raised.is_empty(), "{raised:?}");
    }

    #[test]
    fn one_jump_raises_one_spike_alert() {
        let mut engine = AlertEngine::new(vec![spike_rule(Baseline::MovingAverage)]);
        let start = Instant::now();

        let mut raised = Vec::new();
        for (step, value) in [10.0, 10.0, 10.0, 90.0, 10.0, 10.0].iter().enumerate() {
            raised.extend(engine.evaluate(
                &sample(*value),
                &[],
                start + Duration::from_secs(step as u64),
            ));
        }

        assert_eq!(raised.len(), 1, "{raised:?}");
    }

    #[test]
    fn a_disabled_rule_never_fires() {
        let mut disabled = rule("cpu", 90.0, Duration::from_secs(1));
        disabled.enabled = false;
        let mut engine = AlertEngine::new(vec![disabled]);
        let start = Instant::now();

        let raised = engine.evaluate(&sample(99.0), &[], start + Duration::from_secs(10));

        assert!(raised.is_empty(), "{raised:?}");
    }

    #[test]
    fn a_below_condition_fires_when_the_value_falls() {
        let mut below = rule("mem", 10.0, Duration::from_secs(1));
        below.condition = Condition::Below;
        below.metric = "memory_percent".to_string();
        let mut engine = AlertEngine::new(vec![below]);
        let start = Instant::now();

        let low_memory = SystemSample {
            memory: MemorySample {
                total_bytes: 100,
                used_bytes: 5,
                ..Default::default()
            },
            ..Default::default()
        };
        let _ = engine.evaluate(&low_memory, &[], start);
        let raised = engine.evaluate(&low_memory, &[], start + Duration::from_secs(2));

        assert_eq!(raised.len(), 1);
        assert_eq!(raised[0].category, AlertCategory::Memory);
    }

    #[test]
    fn a_process_metric_names_the_process_it_measured() {
        let mut rule = rule("proc", 50.0, Duration::from_secs(1));
        rule.metric = "process_cpu_percent".to_string();
        let mut engine = AlertEngine::new(vec![rule]);
        let processes = vec![ProcessSample {
            pid: 42,
            name: "backup".into(),
            cpu_percent: 80.0,
            state: ProcessState::Running,
            ..Default::default()
        }];

        let start = Instant::now();
        let _ = engine.evaluate(&SystemSample::default(), &processes, start);
        let raised = engine.evaluate(
            &SystemSample::default(),
            &processes,
            start + Duration::from_secs(2),
        );

        assert_eq!(raised.len(), 1);
        assert_eq!(raised[0].pid, Some(42));
        assert_eq!(raised[0].process_name.as_deref(), Some("backup"));
    }

    #[test]
    fn an_unknown_metric_never_fires() {
        let mut unknown = rule("odd", 1.0, Duration::from_secs(1));
        unknown.metric = "purple".to_string();
        let mut engine = AlertEngine::new(vec![unknown]);

        let raised = engine.evaluate(&sample(99.0), &[], Instant::now());

        assert!(raised.is_empty(), "{raised:?}");
    }
}
