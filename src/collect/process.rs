//! Reading processes out of `/proc`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::core::process::ProcessState;

/// The counters `/proc` reports for one process.
#[derive(Debug, Clone, Default)]
pub struct ProcessCounters {
    pub pid: u32,
    pub ppid: u32,
    /// Start time in jiffies since boot. Together with the PID it identifies
    /// the process, so a reused PID is never mistaken for the original.
    pub start_time: u64,
    pub name: String,
    pub cmd: String,
    pub uid: u32,
    pub user: String,
    pub state: ProcessState,
    pub utime: u64,
    pub stime: u64,
    pub rss_bytes: u64,
    pub vsz_bytes: u64,
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub thread_count: u32,
    pub fd_count: u32,
    pub nice: i32,
    /// The cgroup the process belongs to, when it can be read.
    pub cgroup: Option<String>,
    /// When the process started, in seconds since the epoch.
    pub started_at: u64,
}

/// The fields of `/proc/<pid>/stat` that matter here.
#[derive(Debug, Clone, Copy, Default)]
pub struct StatFields {
    pub state: char,
    pub ppid: u32,
    pub utime: u64,
    pub stime: u64,
    pub num_threads: u32,
    pub start_time: u64,
    pub vsize: u64,
    pub rss_pages: u64,
    pub nice: i32,
}

/// The PIDs of every process on the machine.
pub fn pids(root: &Path) -> Vec<u32> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut pids: Vec<u32> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse().ok())
        })
        .collect();
    pids.sort_unstable();
    pids
}

/// Read one process. `None` means it exited or is not readable.
pub fn read(root: &Path, pid: u32) -> Option<ProcessCounters> {
    let dir = root.join(pid.to_string());
    let stat = std::fs::read_to_string(dir.join("stat")).ok()?;
    let fields = parse_stat(&stat)?;
    let statm = std::fs::read_to_string(dir.join("statm")).ok();
    let (rss_bytes, vsz_bytes) = statm
        .as_deref()
        .map(parse_statm)
        .unwrap_or((fields.rss_pages * PAGE_SIZE, fields.vsize));
    let cmd = std::fs::read(dir.join("cmdline"))
        .map(|bytes| parse_cmdline(bytes.as_slice()))
        .unwrap_or_default();
    let uid = read_uid(&dir).unwrap_or(0);
    let (read_bytes, write_bytes) = std::fs::read_to_string(dir.join("io"))
        .ok()
        .map(|text| parse_io(&text))
        .unwrap_or((0, 0));

    Some(ProcessCounters {
        pid,
        ppid: fields.ppid,
        start_time: fields.start_time,
        name: parse_comm(&stat).unwrap_or_default(),
        cmd,
        uid,
        user: user_name(uid),
        state: parse_state(fields.state),
        utime: fields.utime,
        stime: fields.stime,
        rss_bytes,
        vsz_bytes,
        read_bytes,
        write_bytes,
        thread_count: fields.num_threads,
        fd_count: fd_count(&dir),
        nice: fields.nice,
        cgroup: read_cgroup(&dir),
        started_at: 0,
    })
}

/// The cgroup the process belongs to.
fn read_cgroup(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("cgroup")).ok()?;
    let last = text.lines().rev().find(|line| !line.is_empty())?;
    let path = last.splitn(3, ':').nth(2)?;
    (!path.is_empty()).then(|| path.to_string())
}

const PAGE_SIZE: u64 = 4096;

/// Parse `/proc/<pid>/stat`.
///
/// The command name is in brackets and may contain spaces and brackets of its
/// own, so the fields after it are read from the last closing bracket.
pub fn parse_stat(text: &str) -> Option<StatFields> {
    let close = text.rfind(')')?;
    let rest = text.get(close + 1..)?.trim();
    let values: Vec<&str> = rest.split_whitespace().collect();
    let number = |index: usize| -> Option<u64> { values.get(index)?.parse().ok() };
    Some(StatFields {
        state: values.first().and_then(|v| v.chars().next()).unwrap_or('?'),
        ppid: number(1).unwrap_or(0) as u32,
        utime: number(11).unwrap_or(0),
        stime: number(12).unwrap_or(0),
        num_threads: number(17).unwrap_or(0) as u32,
        start_time: number(19).unwrap_or(0),
        vsize: number(20).unwrap_or(0),
        rss_pages: number(21).unwrap_or(0),
        nice: number(16).unwrap_or(0) as i32,
    })
}

/// The command name in brackets, which may contain spaces.
pub fn parse_comm(text: &str) -> Option<String> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    Some(text.get(open + 1..close)?.to_string())
}

/// Parse `/proc/<pid>/statm`, which counts pages.
pub fn parse_statm(text: &str) -> (u64, u64) {
    let values: Vec<u64> = text
        .split_whitespace()
        .filter_map(|value| value.parse().ok())
        .collect();
    let size = values.first().copied().unwrap_or(0) * PAGE_SIZE;
    let resident = values.get(1).copied().unwrap_or(0) * PAGE_SIZE;
    (resident, size)
}

/// Join a NUL-separated command line. Bytes that are not UTF-8 are replaced
/// rather than rejected: a process with a strange name is still a process.
pub fn parse_cmdline(bytes: &[u8]) -> String {
    let text: Vec<String> = bytes
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8_lossy(part).into_owned())
        .collect();
    text.join(" ")
}

/// Parse the counters in `/proc/<pid>/io`.
pub fn parse_io(text: &str) -> (u64, u64) {
    let mut read_bytes = 0;
    let mut write_bytes = 0;
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim().parse::<u64>().unwrap_or(0);
        match key.trim() {
            "read_bytes" => read_bytes = value,
            "write_bytes" => write_bytes = value,
            _ => {}
        }
    }
    (read_bytes, write_bytes)
}

/// The owner of a process.
fn read_uid(dir: &Path) -> Option<u32> {
    let status = std::fs::read_to_string(dir.join("status")).ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("Uid:") {
            return rest
                .split_whitespace()
                .next()
                .and_then(|value| value.parse().ok());
        }
    }
    None
}

/// How many files the process has open.
fn fd_count(dir: &Path) -> u32 {
    std::fs::read_dir(dir.join("fd"))
        .map(|entries| entries.count() as u32)
        .unwrap_or(0)
}

/// The name of the user owning a UID.
pub fn user_name(uid: u32) -> String {
    unsafe {
        let entry = libc::getpwuid(uid as libc::uid_t);
        if entry.is_null() {
            return uid.to_string();
        }
        let name = (*entry).pw_name;
        if name.is_null() {
            return uid.to_string();
        }
        std::ffi::CStr::from_ptr(name)
            .to_string_lossy()
            .into_owned()
    }
}

/// The state of a process, as `/proc` spells it.
pub fn parse_state(state: char) -> ProcessState {
    match state {
        'R' => ProcessState::Running,
        'S' => ProcessState::Sleeping,
        'D' => ProcessState::DiskSleep,
        'T' => ProcessState::Stopped,
        't' => ProcessState::TracingStop,
        'Z' => ProcessState::Zombie,
        'X' => ProcessState::Dead,
        'I' => ProcessState::Idle,
        _ => ProcessState::Unknown,
    }
}

/// The letter a state is shown as.
pub fn state_letter(state: ProcessState) -> char {
    match state {
        ProcessState::Running => 'R',
        ProcessState::Sleeping => 'S',
        ProcessState::DiskSleep => 'D',
        ProcessState::Stopped => 'T',
        ProcessState::TracingStop => 't',
        ProcessState::Zombie => 'Z',
        ProcessState::Dead => 'X',
        ProcessState::Idle => 'I',
        ProcessState::Unknown => '?',
    }
}

/// Index processes by PID and start time, so a reused PID is not confused with
/// the process that had it before.
pub fn index(counters: &[ProcessCounters]) -> HashMap<(u32, u64), &ProcessCounters> {
    counters
        .iter()
        .map(|process| ((process.pid, process.start_time), process))
        .collect()
}

/// The number of clock ticks per second, which is how Linux counts CPU time.
pub fn clock_ticks() -> f64 {
    static TICKS: OnceLock<f64> = OnceLock::new();
    *TICKS.get_or_init(|| unsafe { libc::sysconf(libc::_SC_CLK_TCK) as f64 })
}

/// Build the per-process samples from two readings.
///
/// A process that was not there in the previous reading shows no CPU usage
/// rather than a spike: it has no history to compare against.
pub fn process_samples(
    current: &[ProcessCounters],
    previous: &[ProcessCounters],
    uptime_secs: f64,
    total_ram: u64,
    elapsed_secs: f64,
) -> Vec<crate::core::process::ProcessSample> {
    let before = index(previous);
    let ticks = clock_ticks();
    current
        .iter()
        .map(|process| {
            let previous = before.get(&(process.pid, process.start_time));
            let cpu_percent = match previous {
                Some(before) if elapsed_secs > 0.0 => {
                    let used =
                        (process.utime + process.stime).saturating_sub(before.utime + before.stime);
                    used as f64 / (ticks * elapsed_secs) * 100.0
                }
                _ => 0.0,
            };
            let (read_rate, write_rate) = match previous {
                Some(before) if elapsed_secs > 0.0 => (
                    (process.read_bytes.saturating_sub(before.read_bytes) as f64 / elapsed_secs)
                        as u64,
                    (process.write_bytes.saturating_sub(before.write_bytes) as f64 / elapsed_secs)
                        as u64,
                ),
                _ => (0, 0),
            };
            let started = process.start_time as f64 / ticks;
            crate::core::process::ProcessSample {
                timestamp: 0,
                pid: process.pid,
                start_time: process.start_time,
                name: process.name.clone(),
                cmd: process.cmd.clone(),
                user: process.user.clone(),
                cpu_percent,
                mem_rss_bytes: process.rss_bytes,
                mem_vsz_bytes: process.vsz_bytes,
                mem_percent: percent(process.rss_bytes, total_ram),
                read_bytes_per_sec: read_rate,
                write_bytes_per_sec: write_rate,
                net_rx_bytes_per_sec: None,
                net_tx_bytes_per_sec: None,
                thread_count: process.thread_count,
                fd_count: process.fd_count,
                state: process.state,
                nice: process.nice,
                pressure_score: 0.0,
                uptime_secs: (uptime_secs - started).max(0.0) as u64,
                ppid: process.ppid,
                started_at: process.started_at,
                cgroup: process.cgroup.clone(),
            }
        })
        .collect()
}

fn percent(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 / whole as f64 * 100.0
    }
}

/// The path of a process directory.
pub fn process_dir(root: &Path, pid: u32) -> PathBuf {
    root.join(pid.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAT: &str = "1234 (my proc (x)) S 42 1234 1234 0 -1 4194304 100 0 0 0 70 12 0 0 20 0 7 0 987654 12345678 456 140737488355328 0 0 0 0 0 0 0 0 0 17 3 0 0 0 0 0\n";

    #[test]
    fn stat_fields_are_read_past_the_command_name() {
        let fields = parse_stat(STAT).unwrap();

        assert_eq!(fields.state, 'S');
        assert_eq!(fields.ppid, 42);
        assert_eq!(fields.utime, 70);
        assert_eq!(fields.stime, 12);
        assert_eq!(fields.num_threads, 7);
        assert_eq!(fields.start_time, 987_654);
        assert_eq!(fields.nice, 0);
    }

    #[test]
    fn the_command_name_may_contain_spaces_and_brackets() {
        assert_eq!(parse_comm(STAT).unwrap(), "my proc (x)");
    }

    #[test]
    fn statm_is_read_in_pages() {
        let (rss, vsz) = parse_statm("1000 250 100 1 0 500 0\n");

        assert_eq!(rss, 250 * 4096);
        assert_eq!(vsz, 1000 * 4096);
    }

    #[test]
    fn a_command_line_that_is_not_utf8_does_not_fail() {
        let bytes = b"ls\xff-l\0--color\0";

        let cmd = parse_cmdline(bytes.as_slice());

        assert!(cmd.starts_with("ls"), "{cmd}");
        assert!(cmd.contains("--color"), "{cmd}");
    }

    #[test]
    fn io_counters_are_read() {
        let (read, write) = parse_io("rchar: 10\nread_bytes: 4096\nwchar: 20\nwrite_bytes: 8192\n");

        assert_eq!(read, 4096);
        assert_eq!(write, 8192);
    }

    #[test]
    fn states_are_spelled_the_way_linux_spells_them() {
        assert_eq!(parse_state('R'), ProcessState::Running);
        assert_eq!(parse_state('Z'), ProcessState::Zombie);
        assert_eq!(parse_state('?'), ProcessState::Unknown);
        assert_eq!(state_letter(ProcessState::Sleeping), 'S');
    }

    #[test]
    fn the_current_process_can_be_read() {
        let counters = read(Path::new("/proc"), std::process::id()).unwrap();

        assert_eq!(counters.pid, std::process::id());
        assert!(counters.start_time > 0);
        assert!(!counters.name.is_empty());
    }
}
