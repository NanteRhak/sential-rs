//! Reading raw counters from the operating system and turning them into rates.
//!
//! Everything here is pure with respect to the operating system: the parsers take
//! text and the collectors take a [`ProcFs`] root, so both can be tested against
//! fixtures without a real `/proc`.

pub mod filesystem;
pub mod proc;
pub mod process;
pub mod sensors;

use std::collections::HashMap;
use std::time::Instant;

use crate::core::format::kb_to_bytes;
use crate::core::metrics::SystemSample;

pub use proc::ProcFs;

/// Raw counters as the operating system reports them, before any differencing.
#[derive(Debug, Clone)]
pub struct RawSnapshot {
    pub at: Instant,
    pub cpu: CpuCounters,
    pub memory: MemoryCounters,
    pub disk: HashMap<String, DiskCounters>,
    pub network: HashMap<String, NetworkCounters>,
    pub load_avg: (f64, f64, f64),
    pub pressure: Option<PressureCounters>,
    pub filesystems: Vec<filesystem::FilesystemCounters>,
    pub notes: Vec<String>,
    pub sensors: sensors::SensorReadings,
    pub processes: Vec<process::ProcessCounters>,
    pub uptime_secs: f64,
}

impl Default for RawSnapshot {
    fn default() -> Self {
        Self {
            at: Instant::now(),
            cpu: CpuCounters::default(),
            memory: MemoryCounters::default(),
            disk: HashMap::new(),
            network: HashMap::new(),
            load_avg: (0.0, 0.0, 0.0),
            pressure: None,
            filesystems: Vec::new(),
            notes: Vec::new(),
            sensors: sensors::SensorReadings::default(),
            processes: Vec::new(),
            uptime_secs: 0.0,
        }
    }
}

/// CPU time counters, in jiffies.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CpuTimes {
    pub user: u64,
    pub nice: u64,
    pub system: u64,
    pub idle: u64,
    pub iowait: u64,
    pub irq: u64,
    pub softirq: u64,
    pub steal: u64,
}

impl CpuTimes {
    pub fn total(&self) -> u64 {
        self.user
            + self.nice
            + self.system
            + self.idle
            + self.iowait
            + self.irq
            + self.softirq
            + self.steal
    }

    /// Time spent doing something rather than waiting.
    pub fn busy(&self) -> u64 {
        self.total() - self.idle - self.iowait
    }
}

#[derive(Debug, Clone, Default)]
pub struct CpuCounters {
    pub total: CpuTimes,
    pub per_core: Vec<CpuTimes>,
    pub context_switches: u64,
    pub interrupts: u64,
}

/// Memory is a gauge, not a counter: these are absolute byte counts.
#[derive(Debug, Clone, Copy, Default)]
pub struct MemoryCounters {
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub cached_bytes: u64,
    pub buffers_bytes: u64,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DiskCounters {
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub read_ops: u64,
    pub write_ops: u64,
    /// Milliseconds spent doing I/O, for latency.
    pub io_ms: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NetworkCounters {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub errors: u64,
    pub drops: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PressureCounters {
    pub cpu_some: f64,
    pub memory_some: f64,
    pub io_some: f64,
}

/// Something that can produce a raw reading.
pub trait Collector: Send {
    /// Read the machine. `Err` means the reading could not be taken at all.
    fn collect(&mut self) -> Result<RawSnapshot, String>;
}

/// The difference between two counter readings.
///
/// A counter that went backwards means the device was replaced or the counter
/// was reset, so the difference is zero rather than a huge wrap-around spike.
pub fn counter_delta(current: u64, previous: u64) -> u64 {
    current.saturating_sub(previous)
}

/// Per-second rate between two counter readings.
pub fn per_second(current: u64, previous: u64, elapsed_secs: f64) -> f64 {
    if elapsed_secs <= 0.0 {
        return 0.0;
    }
    counter_delta(current, previous) as f64 / elapsed_secs
}

/// Busy percentage between two CPU time readings.
pub fn cpu_percent(current: &CpuTimes, previous: &CpuTimes) -> Option<f64> {
    let total = counter_delta(current.total(), previous.total());
    if total == 0 {
        return None;
    }
    let busy = counter_delta(current.busy(), previous.busy());
    Some(busy as f64 / total as f64 * 100.0)
}

/// Percentage of `total` covered by one component of the CPU breakdown.
pub fn cpu_share(current: u64, previous: u64, total_delta: u64) -> f64 {
    if total_delta == 0 {
        return 0.0;
    }
    counter_delta(current, previous) as f64 / total_delta as f64 * 100.0
}

/// Build a sample from a reading and the reading before it.
///
/// Without a previous reading every rate is zero: the first tick has nothing to
/// difference against, and reporting the raw counters would be nonsense.
pub fn sample_between(current: &RawSnapshot, previous: Option<&RawSnapshot>) -> SystemSample {
    let timestamp = now_millis();
    let Some(previous) = previous else {
        return zero_sample(current, timestamp);
    };
    let elapsed = current.at.duration_since(previous.at).as_secs_f64();
    let mut sample = zero_sample(current, timestamp);

    let total_delta = counter_delta(current.cpu.total.total(), previous.cpu.total.total());
    if total_delta > 0 {
        sample.cpu.total_percent =
            cpu_percent(&current.cpu.total, &previous.cpu.total).unwrap_or(0.0);
        sample.cpu.user_percent = cpu_share(
            current.cpu.total.user + current.cpu.total.nice,
            previous.cpu.total.user + previous.cpu.total.nice,
            total_delta,
        );
        sample.cpu.system_percent = cpu_share(
            current.cpu.total.system,
            previous.cpu.total.system,
            total_delta,
        );
        sample.cpu.iowait_percent = cpu_share(
            current.cpu.total.iowait,
            previous.cpu.total.iowait,
            total_delta,
        );
        sample.cpu.idle_percent =
            cpu_share(current.cpu.total.idle, previous.cpu.total.idle, total_delta);
    }
    sample.cpu.per_core_percent = current
        .cpu
        .per_core
        .iter()
        .zip(&previous.cpu.per_core)
        .map(|(now, before)| cpu_percent(now, before).unwrap_or(0.0))
        .collect();

    let mut disk = crate::core::metrics::DiskSample::default();
    for (name, now) in &current.disk {
        let Some(before) = previous.disk.get(name) else {
            continue;
        };
        let device = crate::core::metrics::DiskDeviceSample {
            device: name.clone(),
            read_bytes_per_sec: per_second(now.read_bytes, before.read_bytes, elapsed) as u64,
            write_bytes_per_sec: per_second(now.write_bytes, before.write_bytes, elapsed) as u64,
            read_ops_per_sec: per_second(now.read_ops, before.read_ops, elapsed) as u64,
            write_ops_per_sec: per_second(now.write_ops, before.write_ops, elapsed) as u64,
            latency_ms: None,
        };
        disk.read_bytes_per_sec += device.read_bytes_per_sec;
        disk.write_bytes_per_sec += device.write_bytes_per_sec;
        disk.read_ops_per_sec += device.read_ops_per_sec;
        disk.write_ops_per_sec += device.write_ops_per_sec;
        disk.devices.push(device);
    }
    sample.disk = disk;
    sample.disk.filesystems = current
        .filesystems
        .iter()
        .map(|fs| crate::core::metrics::FilesystemSample {
            mount_point: fs.mount_point.clone(),
            used_bytes: fs.used_bytes,
            total_bytes: fs.total_bytes,
            available_bytes: fs.available_bytes,
        })
        .collect();
    sample.disk.notes = current.notes.clone();

    sample.sensors.temperatures = current
        .sensors
        .temperatures
        .iter()
        .map(|(label, celsius)| crate::core::metrics::TemperatureSample {
            label: label.clone(),
            celsius: *celsius,
        })
        .collect();
    sample.sensors.fans = current
        .sensors
        .fans
        .iter()
        .map(|(label, rpm)| crate::core::metrics::FanSample {
            label: label.clone(),
            rpm: *rpm,
        })
        .collect();
    sample.sensors.throttled = current.sensors.throttled;
    sample.sensors.power_profile = current.sensors.power_profile.clone();
    sample.sensors.pressure =
        current
            .pressure
            .map(|pressure| crate::core::metrics::PressureSample {
                cpu_some_avg10: pressure.cpu_some,
                memory_some_avg10: pressure.memory_some,
                io_some_avg10: pressure.io_some,
            });
    let mut network = crate::core::metrics::NetworkSample::default();
    for (name, now) in &current.network {
        let Some(before) = previous.network.get(name) else {
            continue;
        };
        let interface = crate::core::metrics::InterfaceSample {
            name: name.clone(),
            rx_bytes_per_sec: per_second(now.rx_bytes, before.rx_bytes, elapsed) as u64,
            tx_bytes_per_sec: per_second(now.tx_bytes, before.tx_bytes, elapsed) as u64,
            rx_packets_per_sec: per_second(now.rx_packets, before.rx_packets, elapsed) as u64,
            tx_packets_per_sec: per_second(now.tx_packets, before.tx_packets, elapsed) as u64,
            errors_total: now.errors,
            drops_total: now.drops,
        };
        network.rx_bytes_per_sec += interface.rx_bytes_per_sec;
        network.tx_bytes_per_sec += interface.tx_bytes_per_sec;
        network.rx_packets_per_sec += interface.rx_packets_per_sec;
        network.tx_packets_per_sec += interface.tx_packets_per_sec;
        network.errors_total += interface.errors_total;
        network.drops_total += interface.drops_total;
        network.interfaces.push(interface);
    }
    sample.network = network;

    sample
}

/// A reading with every derived rate set to zero.
fn zero_sample(current: &RawSnapshot, timestamp: u64) -> SystemSample {
    SystemSample {
        timestamp,
        cpu: crate::core::metrics::CpuSample {
            per_core_percent: vec![0.0; current.cpu.per_core.len()],
            load_avg_1: current.load_avg.0,
            load_avg_5: current.load_avg.1,
            load_avg_15: current.load_avg.2,
            context_switches_total: current.cpu.context_switches,
            interrupts_total: current.cpu.interrupts,
            ..Default::default()
        },
        memory: crate::core::metrics::MemorySample {
            total_bytes: current.memory.total_bytes,
            used_bytes: current.memory.total_bytes - current.memory.available_bytes,
            available_bytes: current.memory.available_bytes,
            cached_bytes: current.memory.cached_bytes,
            buffers_bytes: current.memory.buffers_bytes,
            swap_total_bytes: current.memory.swap_total_bytes,
            swap_used_bytes: current.memory.swap_used_bytes,
        },
        ..Default::default()
    }
}

/// Wall-clock milliseconds since the epoch, for display and log records.
pub fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// `kb_to_bytes` re-exported for the parsers in this module.
pub(crate) fn kb(value: u64) -> u64 {
    kb_to_bytes(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn times(user: u64, idle: u64, iowait: u64) -> CpuTimes {
        CpuTimes {
            user,
            system: 0,
            idle,
            iowait,
            ..Default::default()
        }
    }

    #[test]
    fn first_tick_reports_zero_rates() {
        let snapshot = RawSnapshot {
            at: Instant::now(),
            cpu: CpuCounters {
                total: times(100, 900, 0),
                ..Default::default()
            },
            ..Default::default()
        };

        let sample = sample_between(&snapshot, None);

        assert_eq!(sample.cpu.total_percent, 0.0);
        assert_eq!(sample.cpu.user_percent, 0.0);
        assert_eq!(sample.disk.read_bytes_per_sec, 0);
        assert_eq!(sample.network.rx_bytes_per_sec, 0);
    }

    #[test]
    fn rates_are_the_difference_over_the_elapsed_time() {
        let first = RawSnapshot {
            at: Instant::now(),
            disk: HashMap::from([(
                "sda".to_string(),
                DiskCounters {
                    read_bytes: 1_000,
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };
        let mut second = first.clone();
        second.at = first.at + Duration::from_secs(2);
        second.disk.get_mut("sda").unwrap().read_bytes = 3_000;

        let sample = sample_between(&second, Some(&first));

        assert_eq!(sample.disk.read_bytes_per_sec, 1_000);
        assert_eq!(sample.disk.devices[0].device, "sda");
    }

    #[test]
    fn a_counter_reset_reads_as_zero_not_as_a_spike() {
        let delta = counter_delta(10, 100);

        assert_eq!(delta, 0);
        assert_eq!(per_second(10, 100, 1.0), 0.0);
    }

    #[test]
    fn a_zero_delta_produces_a_zero_rate() {
        assert_eq!(per_second(500, 500, 1.0), 0.0);
    }

    #[test]
    fn a_zero_elapsed_time_produces_a_zero_rate() {
        assert_eq!(per_second(500, 100, 0.0), 0.0);
    }

    #[test]
    fn cpu_percentage_uses_the_busy_share_of_the_total() {
        let before = times(100, 900, 0);
        let after = times(200, 1_000, 0);

        let percent = cpu_percent(&after, &before).unwrap();

        assert!((percent - 50.0).abs() < 0.001, "{percent}");
    }

    #[test]
    fn cpu_percentage_is_none_when_no_time_passed() {
        let same = times(100, 900, 0);

        assert_eq!(cpu_percent(&same, &same), None);
    }

    #[test]
    fn the_breakdown_covers_the_whole_tick() {
        let before = CpuTimes {
            user: 100,
            system: 50,
            idle: 800,
            iowait: 50,
            ..Default::default()
        };
        let after = CpuTimes {
            user: 200,
            system: 100,
            idle: 1_600,
            iowait: 100,
            ..Default::default()
        };

        let total = counter_delta(after.total(), before.total());
        let sum = cpu_share(after.user, before.user, total)
            + cpu_share(after.system, before.system, total)
            + cpu_share(after.idle, before.idle, total)
            + cpu_share(after.iowait, before.iowait, total);

        assert!((sum - 100.0).abs() < 0.001, "{sum}");
    }

    #[test]
    fn a_device_that_appeared_between_ticks_is_skipped() {
        let first = RawSnapshot::default();
        let mut second = RawSnapshot::default();
        second
            .disk
            .insert("sdb".to_string(), DiskCounters::default());

        let sample = sample_between(&second, Some(&first));

        assert!(sample.disk.devices.is_empty());
    }
}
