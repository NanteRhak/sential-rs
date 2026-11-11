//! The anomaly engine: rules that watch a metric and raise alerts.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use serde::Serialize;

use crate::config::AlertRule;
use crate::core::Severity;
use crate::core::metrics::SystemSample;
use crate::core::process::ProcessSample;

/// What an alert is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
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
    pub suggestion: String,
}

/// Watches the metrics and raises alerts when a rule's condition holds for
/// long enough.
pub struct AlertEngine {
    rules: Vec<AlertRule>,
    /// When each rule's condition started holding.
    holding_since: HashMap<String, Instant>,
    /// The rules that have already fired and are still holding.
    fired: HashSet<String>,
    next_id: u64,
}

impl AlertEngine {
    pub fn new(rules: Vec<AlertRule>) -> Self {
        Self {
            rules,
            holding_since: HashMap::new(),
            fired: HashSet::new(),
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
        for rule in &self.rules {
            if !rule.enabled {
                continue;
            }
            let Some((value, pid, process_name)) = metric_value(sample, processes, &rule.metric)
            else {
                continue;
            };
            if !rule.condition.holds(value, rule.threshold) {
                self.holding_since.remove(&rule.name);
                self.fired.remove(&rule.name);
                continue;
            }
            let since = *self.holding_since.entry(rule.name.clone()).or_insert(now);
            if now.duration_since(since) < rule.duration {
                continue;
            }
            if self.fired.contains(&rule.name) {
                continue;
            }
            self.fired.insert(rule.clone().name);
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
                threshold: Some(rule.threshold),
                suggestion: category.suggestion().to_string(),
            });
        }
        raised
    }
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
    use crate::config::Condition;
    use crate::core::metrics::{CpuSample, MemorySample};
    use crate::core::process::{ProcessSample, ProcessState};
    use std::time::Duration;

    fn rule(name: &str, threshold: f64, duration: Duration) -> AlertRule {
        AlertRule {
            name: name.to_string(),
            metric: "cpu_percent".to_string(),
            condition: Condition::Above,
            threshold,
            duration,
            severity: Severity::Warning,
            message: "cpu is high".to_string(),
            enabled: true,
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
