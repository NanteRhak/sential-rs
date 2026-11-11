//! Summarising what happened over a window of time.

use std::time::Duration;

use serde::Serialize;

use crate::alert::Alert;
use crate::store::StoredSample;

/// A summary of a window.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// The window that was asked for.
    pub window_secs: u64,
    /// How many samples were inside it.
    pub samples: usize,
    pub cpu: Metric,
    pub memory: Metric,
    pub load: Metric,
    pub disk_read: Metric,
    pub disk_write: Metric,
    pub network_rx: Metric,
    pub network_tx: Metric,
    /// The busiest processes seen in the window.
    pub processes: Vec<ProcessSummary>,
    /// The alerts raised in the window.
    pub alerts: Vec<Alert>,
}

/// One measure over the window.
#[derive(Debug, Clone, Serialize)]
pub struct Metric {
    pub peak: f64,
    pub average: f64,
}

/// One process in the report.
#[derive(Debug, Clone, Serialize)]
pub struct ProcessSummary {
    pub pid: u32,
    pub name: String,
    pub peak_cpu_percent: f64,
    pub peak_mem_percent: f64,
}

impl Report {
    /// Summarise the samples of a window.
    ///
    /// Only the samples inside the window are counted: the history on disk
    /// may well be older than what was asked for.
    pub fn build(samples: &[StoredSample], alerts: &[Alert], window: Duration) -> Report {
        let cutoff = crate::collect::now_millis().saturating_sub(window.as_millis() as u64);
        let inside: Vec<&StoredSample> = samples
            .iter()
            .filter(|sample| sample.timestamp >= cutoff)
            .collect();

        let mut processes: Vec<ProcessSummary> = Vec::new();
        for sample in &inside {
            for process in &sample.processes {
                match processes
                    .iter_mut()
                    .find(|known| known.pid == process.pid && known.name == process.name)
                {
                    Some(known) => {
                        known.peak_cpu_percent = known.peak_cpu_percent.max(process.cpu_percent);
                        known.peak_mem_percent = known.peak_mem_percent.max(process.mem_percent);
                    }
                    None => processes.push(ProcessSummary {
                        pid: process.pid,
                        name: process.name.clone(),
                        peak_cpu_percent: process.cpu_percent,
                        peak_mem_percent: process.mem_percent,
                    }),
                }
            }
        }
        processes.sort_by(|a, b| {
            b.peak_cpu_percent
                .partial_cmp(&a.peak_cpu_percent)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        processes.truncate(10);

        let alerts: Vec<Alert> = alerts
            .iter()
            .filter(|alert| alert.timestamp >= cutoff)
            .cloned()
            .collect();

        Report {
            window_secs: window.as_secs(),
            samples: inside.len(),
            cpu: measure(&inside, |sample| sample.cpu_percent),
            memory: measure(&inside, |sample| sample.memory_percent),
            load: measure(&inside, |sample| sample.load_avg_1),
            disk_read: measure(&inside, |sample| sample.disk_read_bytes_per_sec as f64),
            disk_write: measure(&inside, |sample| sample.disk_write_bytes_per_sec as f64),
            network_rx: measure(&inside, |sample| sample.network_rx_bytes_per_sec as f64),
            network_tx: measure(&inside, |sample| sample.network_tx_bytes_per_sec as f64),
            processes,
            alerts,
        }
    }

    /// The report as JSON, for scripting.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// The peak and the average of a measure over the samples.
fn measure(samples: &[&StoredSample], value: impl Fn(&StoredSample) -> f64) -> Metric {
    if samples.is_empty() {
        return Metric {
            peak: 0.0,
            average: 0.0,
        };
    }
    let values: Vec<f64> = samples.iter().map(|sample| value(sample)).collect();
    Metric {
        peak: values.iter().copied().fold(0.0, f64::max),
        average: values.iter().sum::<f64>() / values.len() as f64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Severity;
    use crate::store::StoredProcess;

    fn sample(timestamp: u64, cpu: f64) -> StoredSample {
        StoredSample {
            timestamp,
            cpu_percent: cpu,
            memory_percent: 50.0,
            load_avg_1: 1.0,
            disk_read_bytes_per_sec: 100,
            disk_write_bytes_per_sec: 200,
            network_rx_bytes_per_sec: 300,
            network_tx_bytes_per_sec: 400,
            processes: vec![StoredProcess {
                pid: 42,
                name: "backup".into(),
                cpu_percent: cpu,
                mem_percent: 5.0,
            }],
        }
    }

    #[test]
    fn the_report_covers_the_window() {
        let now = crate::collect::now_millis();
        let samples = vec![
            sample(now - 60_000, 10.0),
            sample(now - 30_000, 90.0),
            sample(now - 1_000, 30.0),
        ];

        let report = Report::build(&samples, &[], Duration::from_secs(120));

        assert_eq!(report.samples, 3);
        assert!((report.cpu.peak - 90.0).abs() < 0.001);
        assert!((report.cpu.average - 43.33).abs() < 0.1);
    }

    #[test]
    fn samples_outside_the_window_are_left_out() {
        let now = crate::collect::now_millis();
        let samples = vec![sample(now - 600_000, 10.0), sample(now - 1_000, 30.0)];

        let report = Report::build(&samples, &[], Duration::from_secs(60));

        assert_eq!(report.samples, 1);
        assert!((report.cpu.peak - 30.0).abs() < 0.001);
    }

    #[test]
    fn the_report_names_the_busiest_processes() {
        let now = crate::collect::now_millis();
        let samples = vec![sample(now - 1_000, 30.0)];

        let report = Report::build(&samples, &[], Duration::from_secs(60));

        assert_eq!(report.processes.len(), 1);
        assert_eq!(report.processes[0].name, "backup");
        assert!((report.processes[0].peak_cpu_percent - 30.0).abs() < 0.001);
    }

    #[test]
    fn the_report_covers_the_alerts() {
        let now = crate::collect::now_millis();
        let alert = Alert {
            id: 1,
            timestamp: now - 1_000,
            severity: Severity::Warning,
            category: crate::alert::AlertCategory::Cpu,
            message: "cpu is high".into(),
            metric: Some("cpu_percent".into()),
            pid: None,
            process_name: None,
            current_value: 95.0,
            threshold: Some(90.0),
            baseline: None,
            suggestion: "look around".into(),
        };

        let report = Report::build(&[], &[alert], Duration::from_secs(60));

        assert_eq!(report.alerts.len(), 1);
    }

    #[test]
    fn the_report_is_valid_json() {
        let now = crate::collect::now_millis();
        let report = Report::build(&[sample(now, 10.0)], &[], Duration::from_secs(60));

        let value: serde_json::Value = serde_json::from_str(&report.to_json()).unwrap();

        assert_eq!(value["samples"], 1);
        assert!(value["cpu"]["peak"].is_number());
        assert!(value["processes"].is_array());
    }
}
