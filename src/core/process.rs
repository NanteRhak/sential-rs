//! Per-process samples.

#[derive(Debug, Clone)]
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
}

/// The one-letter process states reported by Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProcessState {
    Running,
    Sleeping,
    DiskSleep,
    Stopped,
    TracingStop,
    Zombie,
    Dead,
    Idle,
    #[default]
    Unknown,
}
