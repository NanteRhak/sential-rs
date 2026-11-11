//! One number per process: how much damage it is doing to the machine.

use std::collections::HashMap;

use crate::core::process::ProcessSample;

/// How much each resource counts towards the score.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Weights {
    pub cpu: f64,
    pub memory: f64,
    pub io: f64,
    pub network: f64,
    pub file_descriptors: f64,
    pub restarts: f64,
}

impl Default for Weights {
    fn default() -> Self {
        Self {
            cpu: 1.0,
            memory: 1.0,
            io: 1.0,
            network: 1.0,
            file_descriptors: 1.0,
            restarts: 1.0,
        }
    }
}

/// Score every process against the machine it is running on.
///
/// Each component is measured against the busiest process on this machine, so
/// the score means the same thing on a small machine and a large one: it says
/// how much of the worst there is this process is doing.
pub fn apply_scores(
    processes: &mut [ProcessSample],
    restarts: &HashMap<String, u32>,
    weights: &Weights,
) {
    let max_cpu = peak(processes, |process| process.cpu_percent);
    let max_memory = peak(processes, |process| process.mem_percent);
    let max_io = peak(processes, |process| {
        (process.read_bytes_per_sec + process.write_bytes_per_sec) as f64
    });
    let max_network = peak(processes, |process| {
        (process.net_rx_bytes_per_sec.unwrap_or(0) + process.net_tx_bytes_per_sec.unwrap_or(0))
            as f64
    });
    let max_fds = peak(processes, |process| process.fd_count as f64);
    let max_restarts = restarts.values().copied().max().unwrap_or(0) as f64;

    for process in processes.iter_mut() {
        let restarts = restarts.get(&process.name).copied().unwrap_or(0) as f64;
        process.pressure_score = weights.cpu * share(process.cpu_percent, max_cpu)
            + weights.memory * share(process.mem_percent, max_memory)
            + weights.io
                * share(
                    (process.read_bytes_per_sec + process.write_bytes_per_sec) as f64,
                    max_io,
                )
            + weights.network
                * share(
                    (process.net_rx_bytes_per_sec.unwrap_or(0)
                        + process.net_tx_bytes_per_sec.unwrap_or(0)) as f64,
                    max_network,
                )
            + weights.file_descriptors * share(process.fd_count as f64, max_fds)
            + weights.restarts * share(restarts, max_restarts);
    }
}

/// The largest value any process has for a measure.
fn peak(processes: &[ProcessSample], measure: impl Fn(&ProcessSample) -> f64) -> f64 {
    processes.iter().map(measure).fold(0.0, f64::max)
}

/// How much of the busiest there is this value.
fn share(value: f64, peak: f64) -> f64 {
    if peak <= 0.0 { 0.0 } else { value / peak }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(pid: u32, name: &str, cpu: f64, rss: u64, fds: u32) -> ProcessSample {
        ProcessSample {
            pid,
            name: name.into(),
            cpu_percent: cpu,
            mem_rss_bytes: rss,
            mem_percent: rss as f64 / 1_000_000.0,
            fd_count: fds,
            ..Default::default()
        }
    }

    #[test]
    fn a_process_using_more_gets_a_higher_score() {
        let mut processes = vec![
            process(1, "quiet", 1.0, 1_000, 10),
            process(2, "busy", 50.0, 500_000, 100),
        ];

        apply_scores(&mut processes, &HashMap::new(), &Weights::default());

        assert!(
            processes[1].pressure_score > processes[0].pressure_score,
            "{:?}",
            processes
                .iter()
                .map(|p| p.pressure_score)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_weights_change_the_result() {
        let mut processes = vec![
            process(1, "cpu-hog", 50.0, 1_000, 10),
            process(2, "fd-hog", 1.0, 1_000, 100),
        ];
        let weights = Weights {
            cpu: 1.0,
            memory: 1.0,
            io: 1.0,
            network: 1.0,
            file_descriptors: 0.0,
            restarts: 1.0,
        };

        apply_scores(&mut processes, &HashMap::new(), &weights);
        let with_fds = processes[0].pressure_score - processes[1].pressure_score;

        let weights = Weights {
            file_descriptors: 10.0,
            ..Weights::default()
        };
        apply_scores(&mut processes, &HashMap::new(), &weights);
        let without_fds = processes[0].pressure_score - processes[1].pressure_score;

        assert!(with_fds > 0.0, "{with_fds}");
        assert!(without_fds < with_fds, "{without_fds} vs {with_fds}");
    }

    #[test]
    fn the_score_is_measured_against_the_machine() {
        let mut small = vec![process(1, "a", 50.0, 500, 10)];
        let mut large = vec![
            process(1, "a", 50.0, 500, 10),
            process(2, "b", 100.0, 1_000, 20),
        ];

        apply_scores(&mut small, &HashMap::new(), &Weights::default());
        apply_scores(&mut large, &HashMap::new(), &Weights::default());

        // The same process is half as much of the worst on the busier machine.
        assert!((small[0].pressure_score - large[0].pressure_score).abs() > 0.1);
    }

    #[test]
    fn restarts_count_towards_the_score() {
        let mut processes = vec![process(1, "flapper", 1.0, 1_000, 10)];
        let restarts = HashMap::from([("flapper".to_string(), 5)]);

        apply_scores(&mut processes, &restarts, &Weights::default());

        assert!(processes[0].pressure_score > 0.0);
    }
}
