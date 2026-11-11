//! Per-process samples.

use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
pub struct ProcessSample {
    pub timestamp: u64,
    pub pid: u32,
    pub start_time: u64,
    pub name: String,
    pub cmd: String,
    pub user: String,
    pub cpu_percent: f64,
    pub mem_rss_bytes: u64,
    pub mem_vsz_bytes: u64,
    /// Share of the machine's memory this process holds.
    pub mem_percent: f64,
    pub read_bytes_per_sec: u64,
    pub write_bytes_per_sec: u64,
    /// `None` when the platform cannot attribute network traffic to a process.
    pub net_rx_bytes_per_sec: Option<u64>,
    pub net_tx_bytes_per_sec: Option<u64>,
    pub thread_count: u32,
    pub fd_count: u32,
    pub state: ProcessState,
    pub nice: i32,
    pub pressure_score: f64,
    /// Seconds since the process started.
    pub uptime_secs: u64,
    pub ppid: u32,
    /// When the process started, in seconds since the epoch.
    pub started_at: u64,
    /// The cgroup the process belongs to, when it can be read.
    pub cgroup: Option<String>,
}

/// The one-letter process states reported by Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub enum ProcessState {
    #[serde(rename = "R")]
    Running,
    #[serde(rename = "S")]
    Sleeping,
    #[serde(rename = "D")]
    DiskSleep,
    #[serde(rename = "T")]
    Stopped,
    #[serde(rename = "t")]
    TracingStop,
    #[serde(rename = "Z")]
    Zombie,
    #[serde(rename = "X")]
    Dead,
    #[serde(rename = "I")]
    Idle,
    #[default]
    #[serde(rename = "?")]
    Unknown,
}
