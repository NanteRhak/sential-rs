//! Noticing when a process that was there is not there any more.

use std::collections::HashMap;

use crate::core::process::{ProcessSample, ProcessState};

/// A process that went away.
#[derive(Debug, Clone)]
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
            }
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
}

impl ExitTracker {
    pub fn new() -> Self {
        Self::default()
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
                    gone.push(exit_record(&previous, true));
                }
                continue;
            }
            seen.insert(key, process.clone());
        }

        // The processes that are simply not there any more.
        for (key, process) in &self.seen {
            if !seen.contains_key(key) {
                gone.push(exit_record(process, false));
            }
        }

        self.seen = seen;
        self.exits.extend(gone.iter().cloned());
        if self.exits.len() > 50 {
            self.exits.drain(..self.exits.len() - 50);
        }
        gone
    }
}

/// The record of a process that went away.
fn exit_record(process: &ProcessSample, expected: bool) -> ExitRecord {
    ExitRecord {
        timestamp: crate::collect::now_millis(),
        pid: process.pid,
        ppid: process.ppid,
        process_name: process.name.clone(),
        exit_code: None,
        signal: None,
        // A process that was a zombie had already finished on its own.
        expected,
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
    fn the_exits_are_kept_for_later() {
        let mut tracker = ExitTracker::new();
        tracker.observe(&[process(42, "backup", ProcessState::Running)]);
        tracker.observe(&[]);

        assert_eq!(tracker.exits().len(), 1);
    }
}
