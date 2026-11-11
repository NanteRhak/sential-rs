//! Parsers and readers for the `/proc` files the sampler needs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{
    CpuCounters, CpuTimes, DiskCounters, MemoryCounters, NetworkCounters, PressureCounters,
};

/// The root of a proc filesystem. Defaults to `/proc`; tests point it at fixtures.
#[derive(Debug, Clone)]
pub struct ProcFs {
    root: PathBuf,
}

impl Default for ProcFs {
    fn default() -> Self {
        Self::system()
    }
}

impl ProcFs {
    pub fn system() -> Self {
        Self {
            root: PathBuf::from("/proc"),
        }
    }

    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn read(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(self.root.join(path)).ok()
    }

    pub fn cpu_counters(&self) -> Option<CpuCounters> {
        self.read("stat").and_then(|text| parse_stat(&text))
    }

    pub fn memory(&self) -> Option<MemoryCounters> {
        self.read("meminfo").and_then(|text| parse_meminfo(&text))
    }

    pub fn load_avg(&self) -> Option<(f64, f64, f64)> {
        self.read("loadavg").and_then(|text| parse_loadavg(&text))
    }

    /// Counters for whole disks. Partitions of a disk are left out, or the
    /// totals would count the same bytes twice.
    pub fn disk_counters(&self) -> HashMap<String, DiskCounters> {
        let all = self
            .read("diskstats")
            .map(|text| parse_diskstats(&text))
            .unwrap_or_default();
        let names: Vec<String> = all.keys().cloned().collect();
        all.into_iter()
            .filter(|(name, _)| is_whole_disk(name, &names))
            .collect()
    }

    /// The capacity of every mounted filesystem, and a note for each one that
    /// could not be read.
    pub fn filesystems(&self) -> (Vec<super::filesystem::FilesystemCounters>, Vec<String>) {
        let Some(mounts) = self.read("mounts") else {
            return (Vec::new(), Vec::new());
        };
        let mut filesystems = Vec::new();
        let mut notes = Vec::new();
        for mount_point in super::filesystem::mount_points(&mounts) {
            match super::filesystem::usage(&mount_point) {
                Ok(counters) => filesystems.push(counters),
                Err(error) => notes.push(error),
            }
        }
        (filesystems, notes)
    }

    pub fn network_counters(&self) -> HashMap<String, NetworkCounters> {
        self.read("net/dev")
            .map(|text| parse_netdev(&text))
            .unwrap_or_default()
    }

    pub fn pressure(&self) -> Option<PressureCounters> {
        let cpu = self
            .read("pressure/cpu")
            .and_then(|text| parse_pressure(&text));
        let memory = self
            .read("pressure/memory")
            .and_then(|text| parse_pressure(&text));
        let io = self
            .read("pressure/io")
            .and_then(|text| parse_pressure(&text));
        if cpu.is_none() && memory.is_none() && io.is_none() {
            return None;
        }
        Some(PressureCounters {
            cpu_some: cpu.map(|p| p.cpu_some).unwrap_or(0.0),
            memory_some: memory.map(|p| p.memory_some).unwrap_or(0.0),
            io_some: io.map(|p| p.io_some).unwrap_or(0.0),
        })
    }
}

/// Parse the CPU lines of `/proc/stat`.
pub fn parse_stat(text: &str) -> Option<CpuCounters> {
    let mut counters = CpuCounters::default();
    let mut found = false;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("cpu ") {
            counters.total = parse_cpu_times(rest)?;
            found = true;
        } else if let Some(rest) = line.strip_prefix("cpu") {
            if rest.starts_with(|c: char| c.is_ascii_digit()) {
                counters.per_core.push(parse_cpu_times(rest)?);
            }
        } else if let Some(rest) = line.strip_prefix("intr ") {
            counters.interrupts = first_number(rest);
        } else if let Some(rest) = line.strip_prefix("ctxt ") {
            counters.context_switches = first_number(rest);
        }
    }
    found.then_some(counters)
}

/// Devices that never carry real traffic.
const VIRTUAL_DEVICES: [&str; 5] = ["loop", "ram", "zram", "fd", "sr"];

/// Whether a device is a whole disk rather than a partition of one.
pub fn is_whole_disk(name: &str, all: &[String]) -> bool {
    if VIRTUAL_DEVICES
        .iter()
        .any(|prefix| name.starts_with(prefix))
    {
        return false;
    }
    match parent_device(name) {
        Some(parent) => !all.iter().any(|other| other == parent),
        None => true,
    }
}

/// The disk a partition belongs to: `sda1` belongs to `sda`, `nvme0n1p1` to
/// `nvme0n1`.
fn parent_device(name: &str) -> Option<&str> {
    if let Some((head, tail)) = name.rsplit_once('p')
        && !head.is_empty()
        && !tail.is_empty()
        && tail.chars().all(|c| c.is_ascii_digit())
    {
        return Some(head);
    }
    let head = name.trim_end_matches(|c: char| c.is_ascii_digit());
    (head.len() < name.len()).then_some(head)
}

/// The first number of a line, ignoring whatever follows it.
fn first_number(text: &str) -> u64 {
    text.split_whitespace()
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

/// Parse one `cpu` line: ten fields, of which `guest` is already inside `user`.
pub fn parse_cpu_times(text: &str) -> Option<CpuTimes> {
    let values: Vec<u64> = text
        .split_whitespace()
        .map(|value| value.parse().unwrap_or(0))
        .collect();
    if values.len() < 4 {
        return None;
    }
    Some(CpuTimes {
        user: values[0],
        nice: values[1],
        system: values[2],
        idle: values[3],
        iowait: *values.get(4).unwrap_or(&0),
        irq: *values.get(5).unwrap_or(&0),
        softirq: *values.get(6).unwrap_or(&0),
        steal: *values.get(7).unwrap_or(&0),
    })
}

/// Parse `/proc/meminfo` into byte counts.
pub fn parse_meminfo(text: &str) -> Option<MemoryCounters> {
    let mut counters = MemoryCounters::default();
    let mut total = None;
    let mut available = None;
    let mut cached = 0;
    let mut buffers = 0;
    let mut swap_total = 0;
    let mut swap_free = 0;
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let kilobytes: u64 = value
            .split_whitespace()
            .next()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        match key.trim() {
            "MemTotal" => total = Some(kilobytes),
            "MemAvailable" => available = Some(kilobytes),
            "Cached" => cached = kilobytes,
            "SReclaimable" => cached += kilobytes,
            "Buffers" => buffers = kilobytes,
            "SwapTotal" => swap_total = kilobytes,
            "SwapFree" => swap_free = kilobytes,
            _ => {}
        }
    }
    let total = total?;
    counters.total_bytes = super::kb(total);
    let available = available.unwrap_or(0);
    counters.available_bytes = super::kb(available);
    counters.cached_bytes = super::kb(cached);
    counters.buffers_bytes = super::kb(buffers);
    counters.swap_total_bytes = super::kb(swap_total);
    counters.swap_used_bytes = super::kb(swap_total.saturating_sub(swap_free));
    Some(counters)
}

/// Parse `/proc/loadavg`.
pub fn parse_loadavg(text: &str) -> Option<(f64, f64, f64)> {
    let values: Vec<f64> = text
        .split_whitespace()
        .take(3)
        .map(|value| value.parse().unwrap_or(0.0))
        .collect();
    if values.len() < 3 {
        return None;
    }
    Some((values[0], values[1], values[2]))
}

/// Parse `/proc/diskstats` into per-device counters.
pub fn parse_diskstats(text: &str) -> HashMap<String, DiskCounters> {
    let mut devices = HashMap::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 14 {
            continue;
        }
        let number = |index: usize| fields[index].parse::<u64>().unwrap_or(0);
        devices.insert(
            fields[2].to_string(),
            DiskCounters {
                read_bytes: number(5) * 512,
                write_bytes: number(9) * 512,
                read_ops: number(3),
                write_ops: number(7),
                io_ms: number(12),
            },
        );
    }
    devices
}

/// Parse `/proc/net/dev` into per-interface counters.
pub fn parse_netdev(text: &str) -> HashMap<String, NetworkCounters> {
    let mut interfaces = HashMap::new();
    for line in text.lines() {
        let Some((name, values)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() || name.contains('|') {
            continue;
        }
        let fields: Vec<u64> = values
            .split_whitespace()
            .map(|value| value.parse().unwrap_or(0))
            .collect();
        if fields.len() < 16 {
            continue;
        }
        interfaces.insert(
            name.to_string(),
            NetworkCounters {
                rx_bytes: fields[0],
                rx_packets: fields[1],
                errors: fields[2],
                drops: fields[3],
                tx_bytes: fields[8],
                tx_packets: fields[9],
            },
        );
    }
    interfaces
}

/// Parse a `/proc/pressure/*` file.
pub fn parse_pressure(text: &str) -> Option<PressureCounters> {
    let some = text
        .lines()
        .find(|line| line.starts_with("some "))
        .or_else(|| text.lines().next())?;
    let avg10 = some
        .split_whitespace()
        .find_map(|field| field.strip_prefix("avg10="))
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(0.0);
    Some(PressureCounters {
        cpu_some: avg10,
        memory_some: avg10,
        io_some: avg10,
    })
}

/// Reads the whole machine at every tick.
pub struct ProcCollector {
    fs: ProcFs,
}

impl Default for ProcCollector {
    fn default() -> Self {
        Self::new(ProcFs::system())
    }
}

impl ProcCollector {
    pub fn new(fs: ProcFs) -> Self {
        Self { fs }
    }
}

impl super::Collector for ProcCollector {
    fn collect(&mut self) -> Result<super::RawSnapshot, String> {
        let cpu = self.fs.cpu_counters().ok_or("cannot read /proc/stat")?;
        let memory = self.fs.memory().ok_or("cannot read /proc/meminfo")?;
        let (filesystems, notes) = self.fs.filesystems();
        Ok(super::RawSnapshot {
            at: std::time::Instant::now(),
            cpu,
            memory,
            disk: self.fs.disk_counters(),
            network: self.fs.network_counters(),
            load_avg: self.fs.load_avg().unwrap_or_default(),
            pressure: self.fs.pressure(),
            filesystems,
            notes,
        })
    }
}

/// The name of a file below the proc root, for error messages.
pub fn proc_path(root: &Path, name: &str) -> PathBuf {
    root.join(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_gives_aggregate_and_per_core_times() {
        let text = "cpu  100 20 30 400 50 60 70 80 0 0\ncpu0 50 10 15 200 25 30 35 40 0 0\ncpu1 50 10 15 200 25 30 35 40 0 0\nintr 1234\nctxt 5678\n";

        let counters = parse_stat(text).unwrap();

        assert_eq!(counters.total.user, 100);
        assert_eq!(counters.total.idle, 400);
        assert_eq!(counters.total.iowait, 50);
        assert_eq!(counters.per_core.len(), 2);
        assert_eq!(counters.interrupts, 1234);
        assert_eq!(counters.context_switches, 5678);
    }

    #[test]
    fn stat_reads_the_first_number_of_the_intr_line() {
        let text = "cpu 1 2 3 4 5 6 7 8 0 0\nintr 999 0 0\nctxt 4242\n";

        let counters = parse_stat(text).unwrap();

        assert_eq!(counters.interrupts, 999);
        assert_eq!(counters.context_switches, 4242);
    }

    #[test]
    fn meminfo_gives_byte_counts() {
        let text = "MemTotal:       1000 kB\nMemAvailable:    400 kB\nBuffers:         50 kB\nCached:         200 kB\nSReclaimable:    25 kB\nSwapTotal:      500 kB\nSwapFree:       300 kB\n";

        let counters = parse_meminfo(text).unwrap();

        assert_eq!(counters.total_bytes, 1_024_000);
        assert_eq!(counters.available_bytes, 409_600);
        assert_eq!(counters.cached_bytes, 225 * 1024);
        assert_eq!(counters.buffers_bytes, 51_200);
        assert_eq!(counters.swap_total_bytes, 512_000);
        assert_eq!(counters.swap_used_bytes, 200 * 1024);
    }

    #[test]
    fn loadavg_gives_three_numbers() {
        let (one, five, fifteen) = parse_loadavg("0.52 0.58 0.59 1/1234 5678").unwrap();

        assert!((one - 0.52).abs() < 0.001);
        assert!((five - 0.58).abs() < 0.001);
        assert!((fifteen - 0.59).abs() < 0.001);
    }

    #[test]
    fn diskstats_turns_sectors_into_bytes() {
        let text = "   8       0 sda 100 0 200 10 50 0 400 20 0 30 40\n";

        let devices = parse_diskstats(text);
        let sda = devices.get("sda").unwrap();

        assert_eq!(sda.read_bytes, 200 * 512);
        assert_eq!(sda.write_bytes, 400 * 512);
        assert_eq!(sda.read_ops, 100);
        assert_eq!(sda.write_ops, 50);
        assert_eq!(sda.io_ms, 30);
    }

    #[test]
    fn netdev_skips_the_header_and_reads_both_directions() {
        let text = "Inter-|   Receive\n face |bytes packets\n    lo: 10 1 0 0 0 0 0 0 20 2 0 0 0 0 0 0\n  eth0: 100 3 1 2 0 0 0 0 400 5 0 0 0 0 0 0\n";

        let interfaces = parse_netdev(text);

        let eth0 = interfaces.get("eth0").unwrap();
        assert_eq!(eth0.rx_bytes, 100);
        assert_eq!(eth0.tx_bytes, 400);
        assert_eq!(eth0.errors, 1);
        assert_eq!(eth0.drops, 2);
        assert_eq!(interfaces.len(), 2);
    }

    #[test]
    fn pressure_reads_the_some_line() {
        let counters = parse_pressure("some avg10=1.50 avg60=0.20 avg300=0.01 total=42\n").unwrap();

        assert!((counters.cpu_some - 1.5).abs() < 0.001);
    }

    #[test]
    fn partitions_are_left_out_of_the_disk_counters() {
        let all: Vec<String> = ["sda", "sda1", "nvme0n1", "nvme0n1p1", "loop0"]
            .iter()
            .map(|name| name.to_string())
            .collect();

        assert!(is_whole_disk("sda", &all));
        assert!(!is_whole_disk("sda1", &all));
        assert!(is_whole_disk("nvme0n1", &all));
        assert!(!is_whole_disk("nvme0n1p1", &all));
        assert!(!is_whole_disk("loop0", &all));
    }

    #[test]
    fn a_short_cpu_line_is_not_a_reading() {
        assert_eq!(parse_cpu_times("1 2 3"), None);
    }
}
