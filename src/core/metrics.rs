//! System-wide samples: one point in time across every subsystem.

#[derive(Debug, Clone, Default)]
pub struct SystemSample {
    pub timestamp: u64,
    pub cpu: CpuSample,
    pub memory: MemorySample,
    pub disk: DiskSample,
    pub network: NetworkSample,
    pub sensors: SensorSample,
}

#[derive(Debug, Clone, Default)]
pub struct CpuSample {
    pub total_percent: f64,
    pub per_core_percent: Vec<f64>,
    pub user_percent: f64,
    pub system_percent: f64,
    pub iowait_percent: f64,
    pub idle_percent: f64,
    pub load_avg_1: f64,
    pub load_avg_5: f64,
    pub load_avg_15: f64,
    pub freq_mhz: Option<u64>,
    pub context_switches_total: u64,
    pub interrupts_total: u64,
}

#[derive(Debug, Clone, Default)]
pub struct MemorySample {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub cached_bytes: u64,
    pub buffers_bytes: u64,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
}

#[derive(Debug, Clone, Default)]
pub struct DiskSample {
    pub read_bytes_per_sec: u64,
    pub write_bytes_per_sec: u64,
    pub read_ops_per_sec: u64,
    pub write_ops_per_sec: u64,
    pub devices: Vec<DiskDeviceSample>,
    pub filesystems: Vec<FilesystemSample>,
    /// Filesystems that could not be read, and why.
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DiskDeviceSample {
    pub device: String,
    pub read_bytes_per_sec: u64,
    pub write_bytes_per_sec: u64,
    pub read_ops_per_sec: u64,
    pub write_ops_per_sec: u64,
    pub latency_ms: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct FilesystemSample {
    pub mount_point: String,
    pub used_bytes: u64,
    pub total_bytes: u64,
    pub available_bytes: u64,
}

#[derive(Debug, Clone, Default)]
pub struct NetworkSample {
    pub rx_bytes_per_sec: u64,
    pub tx_bytes_per_sec: u64,
    pub rx_packets_per_sec: u64,
    pub tx_packets_per_sec: u64,
    pub errors_total: u64,
    pub drops_total: u64,
    pub interfaces: Vec<InterfaceSample>,
}

#[derive(Debug, Clone, Default)]
pub struct InterfaceSample {
    pub name: String,
    pub rx_bytes_per_sec: u64,
    pub tx_bytes_per_sec: u64,
    pub rx_packets_per_sec: u64,
    pub tx_packets_per_sec: u64,
    pub errors_total: u64,
    pub drops_total: u64,
}

#[derive(Debug, Clone, Default)]
pub struct SensorSample {
    pub temperatures: Vec<TemperatureSample>,
    pub fans: Vec<FanSample>,
    pub power_profile: Option<String>,
    /// `None` when the platform does not report throttling.
    pub throttled: Option<bool>,
    /// Kernel pressure-stall information, when exposed.
    pub pressure: Option<PressureSample>,
}

#[derive(Debug, Clone, Default)]
pub struct TemperatureSample {
    pub label: String,
    pub celsius: f64,
}

#[derive(Debug, Clone, Default)]
pub struct FanSample {
    pub label: String,
    pub rpm: u64,
}

#[derive(Debug, Clone, Default)]
pub struct PressureSample {
    pub cpu_some_avg10: f64,
    pub memory_some_avg10: f64,
    pub io_some_avg10: f64,
}
