//! Looking for evidence that the OOM killer took a process.
//!
//! The kernel log is where that evidence lives, and reading it needs
//! privileges most people do not have. When it cannot be read the tool says
//! nothing rather than guessing.

use std::process::Command;

/// How many lines of the kernel log are looked at.
const LINES: usize = 400;

/// The evidence that the OOM killer took a process, if there is any.
pub fn evidence_for(pid: u32) -> Option<String> {
    let log = kernel_log()?;
    evidence_in_log(&log, pid)
}

/// The line in the log that names the process, if there is one.
fn evidence_in_log(log: &str, pid: u32) -> Option<String> {
    let killed = format!("Killed process {pid}");
    let pid = pid.to_string();
    log.lines()
        .rev()
        .take(LINES)
        .find(|line| {
            line.contains(&killed)
                || (line.contains("oom-kill") && line.contains(&pid))
                || (line.contains("Out of memory: Killed process") && line.contains(&pid))
        })
        .map(|line| line.trim().to_string())
}

/// The kernel log, if it can be read.
fn kernel_log() -> Option<String> {
    let output = Command::new("dmesg").output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_log_without_the_process_has_no_evidence() {
        assert!(evidence_in_log("some other message", 42).is_none());
    }

    #[test]
    fn the_line_that_names_the_process_is_the_evidence() {
        let log =
            "Mon Jan  1 00:00:00 2024: Out of memory: Killed process 42 (backup) total-vm:100kB";

        let evidence = evidence_in_log(log, 42);

        assert!(evidence.is_some(), "{evidence:?}");
        assert!(evidence.unwrap().contains("Killed process 42"));
    }

    #[test]
    fn an_oom_kill_line_is_the_evidence() {
        let log = "Mon Jan  1 00:00:00 2024: memory:oom-kill:process=42,pid=42";

        assert!(evidence_in_log(log, 42).is_some());
    }

    #[test]
    fn a_log_that_cannot_be_read_says_nothing() {
        // There is no kernel log to read in most build environments, and that
        // must not be an error: the tool says nothing rather than guessing.
        if std::process::Command::new("dmesg").output().is_err() {
            assert!(evidence_for(42).is_none());
        }
    }
}
