//! Acting on processes: signalling them, and recording what was done.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// One action that was taken, or refused.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRecord {
    pub timestamp: u64,
    pub action: String,
    pub pid: u32,
    pub process_name: String,
    pub reason: String,
    /// Whether the user asked for this, rather than a policy.
    pub user_requested: bool,
    /// What happened.
    pub result: String,
}

/// Send a signal to a process.
pub fn send_signal(pid: u32, signal: i32) -> Result<(), String> {
    let result = unsafe { libc::kill(pid as libc::pid_t, signal) };
    if result != 0 {
        return Err(format!(
            "cannot signal process {pid}: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

/// The start time of a process, in jiffies since boot.
///
/// A PID on its own is not an identity: the kernel reuses them. The start
/// time tells one process from the next one to get the same number.
pub fn start_time(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    crate::collect::process::parse_stat(&stat).map(|fields| fields.start_time)
}

/// The name of a process.
pub fn process_name(pid: u32) -> Option<String> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    crate::collect::process::parse_comm(&stat)
}

/// Whether a process is protected from being touched.
pub fn protection(name: &str, protected: &[String]) -> Option<String> {
    protected
        .iter()
        .find(|rule| *rule == name || name.starts_with(rule.as_str()))
        .cloned()
}

/// Append an action to the action log, one JSON object per line.
pub fn write_record(path: &Path, record: &ActionRecord) -> Result<(), String> {
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
    let line = serde_json::to_string(record).map_err(|error| error.to_string())?;
    writeln!(file, "{line}").map_err(|error| format!("{}: {error}", path.display()))
}

/// Read the action log, oldest first.
pub fn read_records(path: &Path) -> Vec<ActionRecord> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| serde_json::from_str::<ActionRecord>(line).ok())
        .collect()
}

/// Where the action log is written.
pub fn log_path(configured: Option<&PathBuf>) -> Option<PathBuf> {
    configured.cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_protected_process_is_recognised_by_name() {
        let protected = ["systemd".to_string(), "sshd".to_string()];

        assert_eq!(protection("systemd", &protected), Some("systemd".into()));
        assert_eq!(protection("sshd", &protected), Some("sshd".into()));
        assert_eq!(protection("firefox", &protected), None);
    }

    #[test]
    fn a_process_name_is_read_from_proc() {
        let name = process_name(std::process::id());

        assert!(name.is_some());
    }

    #[test]
    fn the_action_log_round_trips() {
        let path =
            std::env::temp_dir().join(format!("sentinel-actions-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let record = ActionRecord {
            timestamp: 1_700_000_000_000,
            action: "kill".into(),
            pid: 42,
            process_name: "backup".into(),
            reason: "user asked".into(),
            user_requested: true,
            result: "sent SIGTERM".into(),
        };

        write_record(&path, &record).unwrap();
        let records = read_records(&path);
        let _ = std::fs::remove_file(&path);

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].pid, 42);
        assert_eq!(records[0].action, "kill");
        assert!(records[0].user_requested);
    }

    #[test]
    fn a_process_that_is_not_there_has_no_start_time() {
        assert_eq!(start_time(999_999_999), None);
    }
}
