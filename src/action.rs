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
    crate::store::append_json(path, record)
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

/// Performs the actions a policy proposes.
pub struct Executor {
    protected: Vec<String>,
    log_file: Option<PathBuf>,
    dry_run: bool,
}

impl Executor {
    pub fn new(protected: Vec<String>, log_file: Option<PathBuf>, dry_run: bool) -> Self {
        Self {
            protected,
            log_file,
            dry_run,
        }
    }

    /// Whether the process is protected from being touched.
    pub fn protection(&self, name: &str) -> Option<String> {
        protection(name, &self.protected)
    }

    /// Do what the proposal says, and write down what happened.
    ///
    /// A protected process is not terminated unless the policy says it may be.
    pub fn perform(&self, proposal: &crate::policy::Proposal, allow_protected: bool) -> String {
        let mut record = ActionRecord {
            timestamp: crate::collect::now_millis(),
            action: proposal.action.label().to_string(),
            pid: proposal.pid,
            process_name: proposal.process_name.clone(),
            reason: format!("policy {}", proposal.policy),
            user_requested: false,
            result: String::new(),
        };

        if let Some(rule) = self.protection(&proposal.process_name)
            && !allow_protected
        {
            record.result = format!("refused: protected by the {rule} rule");
            self.write(&record);
            return record.result.clone();
        }

        if proposal.require_confirmation {
            record.result = "waiting for confirmation".to_string();
            self.write(&record);
            return record.result.clone();
        }

        if self.dry_run {
            record.result = "dry run: nothing was done".to_string();
            self.write(&record);
            return record.result.clone();
        }

        record.result = match proposal.action {
            crate::policy::PolicyAction::Kill => match send_signal(proposal.pid, libc::SIGTERM) {
                Ok(()) => "sent SIGTERM".to_string(),
                Err(error) => format!("failed: {error}"),
            },
            crate::policy::PolicyAction::Suspend => {
                match send_signal(proposal.pid, libc::SIGSTOP) {
                    Ok(()) => "sent SIGSTOP".to_string(),
                    Err(error) => format!("failed: {error}"),
                }
            }
            crate::policy::PolicyAction::Resume => match send_signal(proposal.pid, libc::SIGCONT) {
                Ok(()) => "sent SIGCONT".to_string(),
                Err(error) => format!("failed: {error}"),
            },
            crate::policy::PolicyAction::Renice => match renice(proposal.pid, 5) {
                Ok(()) => "priority set to 5".to_string(),
                Err(error) => format!("failed: {error}"),
            },
            crate::policy::PolicyAction::Throttle => "not supported on this platform".to_string(),
        };
        self.write(&record);
        record.result.clone()
    }

    /// Write an action to the action log.
    fn write(&self, record: &ActionRecord) {
        let Some(path) = &self.log_file else {
            return;
        };
        if let Err(error) = write_record(path, record) {
            eprintln!("sentinel: {error}");
        }
    }
}

/// The signals that can be sent by name.
pub const SIGNALS: [(&str, i32); 10] = [
    ("HUP", libc::SIGHUP),
    ("INT", libc::SIGINT),
    ("TERM", libc::SIGTERM),
    ("KILL", libc::SIGKILL),
    ("STOP", libc::SIGSTOP),
    ("CONT", libc::SIGCONT),
    ("USR1", libc::SIGUSR1),
    ("USR2", libc::SIGUSR2),
    ("QUIT", libc::SIGQUIT),
    ("ALRM", libc::SIGALRM),
];

/// A signal by name, such as `SIGTERM`, or by number.
pub fn parse_signal(text: &str) -> Result<i32, String> {
    let text = text.trim();
    let name = text.trim_start_matches("SIG").to_uppercase();
    if let Some((_, signal)) = SIGNALS.iter().find(|(known, _)| *known == name) {
        return Ok(*signal);
    }
    text.parse::<i32>()
        .map_err(|_| format!("unknown signal `{text}`"))
}

/// The name of a signal, for messages.
pub fn signal_name(signal: i32) -> String {
    match SIGNALS.iter().find(|(_, known)| *known == signal) {
        Some((name, _)) => format!("SIG{name}"),
        None => format!("signal {signal}"),
    }
}

/// Change the priority of a process. Lower is more important.
pub fn renice(pid: u32, value: i32) -> Result<(), String> {
    let result = unsafe { libc::setpriority(libc::PRIO_PROCESS, pid, value) };
    if result != 0 {
        let error = std::io::Error::last_os_error();
        let reason = match error.raw_os_error() {
            Some(libc::EACCES) | Some(libc::EPERM) => {
                "you do not have permission to change its priority".to_string()
            }
            _ => error.to_string(),
        };
        return Err(format!("cannot renice process {pid}: {reason}"));
    }
    Ok(())
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

    fn proposal(name: &str, pid: u32) -> crate::policy::Proposal {
        crate::policy::Proposal {
            policy: "kill-hog".into(),
            pid,
            process_name: name.into(),
            action: crate::policy::PolicyAction::Kill,
            reason: "the policy matched".into(),
            current_value: 90.0,
            allow_protected: false,
            require_confirmation: false,
        }
    }

    #[test]
    fn an_automatic_policy_refuses_a_protected_process() {
        let executor = Executor::new(vec!["sshd".into()], None, false);

        let result = executor.perform(&proposal("sshd", 1), false);

        assert!(result.contains("refused"), "{result}");
        assert!(result.contains("sshd"), "{result}");
    }

    #[test]
    fn an_automatic_policy_may_act_on_an_unprotected_process() {
        let executor = Executor::new(vec!["sshd".into()], None, false);

        // The process is one we cannot signal in the test, but the policy is
        // allowed to try: the refusal must not be about protection.
        let result = executor.perform(&proposal("sleep", 999_999), false);

        assert!(!result.contains("refused: protected"), "{result}");
    }

    #[test]
    fn a_policy_that_is_allowed_may_act_on_a_protected_process() {
        let executor = Executor::new(vec!["sshd".into()], None, false);

        let result = executor.perform(&proposal("sshd", 999_999), true);

        assert!(!result.contains("refused: protected"), "{result}");
    }

    #[test]
    fn a_dry_run_does_nothing() {
        let executor = Executor::new(Vec::new(), None, true);

        let result = executor.perform(&proposal("sleep", 999_999), false);

        assert!(result.contains("dry run"), "{result}");
    }

    #[test]
    fn an_automatic_action_is_written_to_the_action_log() {
        let path = std::env::temp_dir().join(format!("sentinel-auto-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let executor = Executor::new(Vec::new(), Some(path.clone()), true);

        executor.perform(&proposal("backup", 42), false);
        let records = read_records(&path);
        let _ = std::fs::remove_file(&path);

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].action, "kill");
        assert_eq!(records[0].pid, 42);
        assert!(records[0].reason.contains("kill-hog"), "{:?}", records[0]);
        assert!(!records[0].user_requested);
    }

    #[test]
    fn a_process_that_is_not_there_has_no_start_time() {
        assert_eq!(start_time(999_999_999), None);
    }
}
