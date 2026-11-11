//! Noticing when a process that was there is not there any more.

use std::collections::HashMap;

use serde::Serialize;

use crate::core::process::{ProcessSample, ProcessState};

/// A process that went away.
#[derive(Debug, Clone, Serialize)]
pub struct ExitRecord {
    pub timestamp: u64,
    pub pid: u32,
    pub ppid: u32,
    pub process_name: String,
    /// The exit code, when it could be read.
    pub exit_code: Option<i32>,
    /// The signal that killed it, when it could be read.
    pub signal: Option<i32>,
    /// Whether the exit looked like a normal one.
    pub expected: bool,
    /// Whether the OOM killer is likely to have taken it.
    pub oom_killed: bool,
    /// The line of the kernel log that says so.
    pub evidence: Option<String>,
    /// What the process was using when it was last seen.
    pub last_cpu_percent: f64,
    pub last_mem_rss_bytes: u64,
    pub last_mem_percent: f64,
    pub last_uptime_secs: u64,
}

impl ExitRecord {
    /// Why the process went away, in words.
    pub fn reason(&self) -> String {
        match (self.exit_code, self.signal) {
            (Some(code), _) => format!("exited with code {code}"),
            (None, Some(signal)) => format!("killed by signal {signal}"),
            (None, None) if self.expected => {
                "exited normally, the parent had not taken it away yet".to_string()
            }
            (None, None) => {
                "disappeared without being seen as a zombie, so it was probably killed".to_string()
            } // The kernel only tells the parent of a process what its exit
              // code was, and this tool is not the parent.
        }
    }
}

/// Watches the processes from one tick to the next.
#[derive(Default)]
pub struct ExitTracker {
    /// The processes seen in the last tick, by PID and start time.
    seen: HashMap<(u32, u64), ProcessSample>,
    /// The processes that went away.
    exits: Vec<ExitRecord>,
    /// The recent exits of each process name, for restart loops.
    restarts: HashMap<String, Vec<u64>>,
    /// How many restarts in the window count as a restart loop.
    restart_count: u32,
    /// The window the restarts are counted in.
    restart_window: std::time::Duration,
    /// Whether to look for OOM evidence.
    oom_evidence: bool,
}

/// A process that keeps coming back.
#[derive(Debug, Clone, Serialize)]
pub struct RestartLoop {
    pub process_name: String,
    pub count: u32,
    pub window_secs: u64,
    pub first_seen: u64,
}

impl ExitTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Look for restart loops, and for OOM evidence.
    pub fn with_detection(
        mut self,
        restart_count: u32,
        restart_window: std::time::Duration,
        oom_evidence: bool,
    ) -> Self {
        self.restart_count = restart_count;
        self.restart_window = restart_window;
        self.oom_evidence = oom_evidence;
        self
    }

    /// How many times each process has been seen to exit.
    pub fn restart_counts(&self) -> HashMap<String, u32> {
        self.restarts
            .iter()
            .map(|(name, times)| (name.clone(), times.len() as u32))
            .collect()
    }

    /// The processes that keep coming back.
    pub fn restart_loops(&self) -> Vec<RestartLoop> {
        let window = self.restart_window.as_millis() as u64;
        let now = crate::collect::now_millis();
        self.restarts
            .iter()
            .filter(|(_, times)| times.len() as u32 >= self.restart_count)
            .map(|(name, times)| RestartLoop {
                process_name: name.clone(),
                count: times.len() as u32,
                window_secs: window / 1_000,
                first_seen: times.first().copied().unwrap_or(now),
            })
            .collect()
    }

    /// The exits noticed so far, oldest first.
    pub fn exits(&self) -> &[ExitRecord] {
        &self.exits
    }

    /// Look at a new set of processes and notice the ones that are gone.
    ///
    /// A process is identified by its PID and start time, so a PID that was
    /// reused is not mistaken for the process that had it before.
    pub fn observe(&mut self, processes: &[ProcessSample]) -> Vec<ExitRecord> {
        let mut gone = Vec::new();
        let mut seen = HashMap::new();

        for process in processes {
            let key = (process.pid, process.start_time);
            if process.state == ProcessState::Zombie {
                // It has finished, and its parent has not taken it away yet.
                if let Some(previous) = self.seen.remove(&key) {
                    gone.push(exit_record(&previous, true, self.oom_evidence));
                }
                continue;
            }
            seen.insert(key, process.clone());
        }

        // The processes that are simply not there any more.
        for (key, process) in &self.seen {
            if !seen.contains_key(key) {
                gone.push(exit_record(process, false, self.oom_evidence));
            }
        }

        self.seen = seen;

        let window = self.restart_window.as_millis() as u64;
        let now = crate::collect::now_millis();
        for exit in &gone {
            let times = self.restarts.entry(exit.process_name.clone()).or_default();
            times.push(exit.timestamp);
            times.retain(|at| now.saturating_sub(*at) <= window);
        }

        self.exits.extend(gone.iter().cloned());
        if self.exits.len() > 50 {
            self.exits.drain(..self.exits.len() - 50);
        }
        gone
    }
}

/// The record of a process that went away.
fn exit_record(process: &ProcessSample, expected: bool, oom_evidence: bool) -> ExitRecord {
    let evidence = if oom_evidence && !expected {
        crate::oom::evidence_for(process.pid)
    } else {
        None
    };
    ExitRecord {
        timestamp: crate::collect::now_millis(),
        pid: process.pid,
        ppid: process.ppid,
        process_name: process.name.clone(),
        exit_code: None,
        signal: None,
        // A process that was a zombie had already finished on its own.
        expected,
        oom_killed: evidence.is_some(),
        evidence,
        last_cpu_percent: process.cpu_percent,
        last_mem_rss_bytes: process.mem_rss_bytes,
        last_mem_percent: process.mem_percent,
        last_uptime_secs: process.uptime_secs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(pid: u32, name: &str, state: ProcessState) -> ProcessSample {
        ProcessSample {
            pid,
            start_time: 100,
            name: name.into(),
            state,
            cpu_percent: 5.0,
            mem_rss_bytes: 1024,
            mem_percent: 1.0,
            ..Default::default()
        }
    }

    #[test]
    fn a_process_that_goes_away_is_noticed() {
        let mut tracker = ExitTracker::new();
        tracker.observe(&[process(42, "backup", ProcessState::Running)]);

        let gone = tracker.observe(&[]);

        assert_eq!(gone.len(), 1);
        assert_eq!(gone[0].pid, 42);
        assert_eq!(gone[0].process_name, "backup");
        assert_eq!(gone[0].ppid, 0);
        assert!(!gone[0].expected);
        assert!(gone[0].reason().contains("killed"), "{}", gone[0].reason());
    }

    #[test]
    fn the_usage_before_the_exit_is_kept() {
        let mut tracker = ExitTracker::new();
        tracker.observe(&[process(42, "backup", ProcessState::Running)]);

        let gone = tracker.observe(&[]);

        assert_eq!(gone[0].last_cpu_percent, 5.0);
        assert_eq!(gone[0].last_mem_rss_bytes, 1024);
        assert_eq!(gone[0].last_mem_percent, 1.0);
    }

    #[test]
    fn a_process_that_is_still_there_is_not_an_exit() {
        let mut tracker = ExitTracker::new();
        tracker.observe(&[process(42, "backup", ProcessState::Running)]);

        let gone = tracker.observe(&[process(42, "backup", ProcessState::Running)]);

        assert!(gone.is_empty());
    }

    #[test]
    fn a_process_that_became_a_zombie_exited_normally() {
        let mut tracker = ExitTracker::new();
        tracker.observe(&[process(42, "backup", ProcessState::Running)]);

        let gone = tracker.observe(&[process(42, "backup", ProcessState::Zombie)]);

        assert_eq!(gone.len(), 1);
        assert!(gone[0].expected);
        assert!(
            gone[0].reason().contains("normally"),
            "{}",
            gone[0].reason()
        );
    }

    #[test]
    fn a_zombie_is_only_reported_once() {
        let mut tracker = ExitTracker::new();
        tracker.observe(&[process(42, "backup", ProcessState::Running)]);

        let first = tracker.observe(&[process(42, "backup", ProcessState::Zombie)]);
        let second = tracker.observe(&[process(42, "backup", ProcessState::Zombie)]);

        assert_eq!(first.len(), 1);
        assert!(second.is_empty());
    }

    #[test]
    fn a_reused_pid_is_not_the_same_process() {
        let mut tracker = ExitTracker::new();
        let mut first = process(42, "backup", ProcessState::Running);
        first.start_time = 100;
        tracker.observe(&[first]);

        let mut second = process(42, "other", ProcessState::Running);
        second.start_time = 200;
        let gone = tracker.observe(&[second]);

        assert_eq!(gone.len(), 1);
        assert_eq!(gone[0].process_name, "backup");
    }

    #[test]
    fn a_process_that_keeps_coming_back_is_a_restart_loop() {
        let mut tracker =
            ExitTracker::new().with_detection(3, std::time::Duration::from_secs(60), false);

        for step in 0..3 {
            tracker.observe(&[process(42, "backup", ProcessState::Running)]);
            tracker.observe(&[]);
            let _ = step;
        }

        let loops = tracker.restart_loops();

        assert_eq!(loops.len(), 1, "{loops:?}");
        assert_eq!(loops[0].process_name, "backup");
        assert_eq!(loops[0].count, 3);
    }

    #[test]
    fn a_process_that_restarts_once_is_not_a_restart_loop() {
        let mut tracker =
            ExitTracker::new().with_detection(3, std::time::Duration::from_secs(60), false);

        tracker.observe(&[process(42, "backup", ProcessState::Running)]);
        tracker.observe(&[]);

        assert!(tracker.restart_loops().is_empty());
    }

    #[test]
    fn the_exits_are_kept_for_later() {
        let mut tracker = ExitTracker::new();
        tracker.observe(&[process(42, "backup", ProcessState::Running)]);
        tracker.observe(&[]);

        assert_eq!(tracker.exits().len(), 1);
    }
}
