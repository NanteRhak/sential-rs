//! Text views: the one-screen summaries the CLI prints.

use crate::core::format::bytes_to_human;
use crate::core::metrics::SystemSample;

const LABEL: usize = 18;

/// The `sentinel health` screen.
pub fn health(sample: &SystemSample) -> String {
    let mut out = String::new();
    out.push_str("CPU\n");
    out.push_str(&row(
        "Total",
        &format!(
            "{:.1}%  user {:.1}%  system {:.1}%  iowait {:.1}%  idle {:.1}%",
            sample.cpu.total_percent,
            sample.cpu.user_percent,
            sample.cpu.system_percent,
            sample.cpu.iowait_percent,
            sample.cpu.idle_percent
        ),
    ));
    out.push_str(&row(
        "Per-core",
        &join_percent(&sample.cpu.per_core_percent),
    ));
    out.push_str(&row(
        "Load average",
        &format!(
            "{:.2} {:.2} {:.2}",
            sample.cpu.load_avg_1, sample.cpu.load_avg_5, sample.cpu.load_avg_15
        ),
    ));
    out.push_str(&row(
        "Context switches",
        &format!(
            "{}  interrupts {}",
            sample.cpu.context_switches_total, sample.cpu.interrupts_total
        ),
    ));

    out.push('\n');
    out.push_str("Memory\n");
    let total = sample.memory.total_bytes;
    let used = sample.memory.used_bytes;
    let used_percent = percent(used, total);
    out.push_str(&row("Total", &bytes_to_human(total)));
    out.push_str(&row(
        "Used",
        &format!("{} ({used_percent:.1}%)", bytes_to_human(used)),
    ));
    out.push_str(&row(
        "Available",
        &bytes_to_human(sample.memory.available_bytes),
    ));
    out.push_str(&row("Cached", &bytes_to_human(sample.memory.cached_bytes)));
    out.push_str(&row(
        "Buffers",
        &bytes_to_human(sample.memory.buffers_bytes),
    ));
    out.push_str(&row(
        "Swap",
        &format!(
            "{} total, {} used",
            bytes_to_human(sample.memory.swap_total_bytes),
            bytes_to_human(sample.memory.swap_used_bytes)
        ),
    ));
    out.push('\n');
    out.push_str(&disk_section(sample));
    out
}

/// The disk row: throughput, the per-disk breakdown and filesystem usage.
fn disk_section(sample: &SystemSample) -> String {
    let disk = &sample.disk;
    let mut out = String::new();
    out.push_str("Disk\n");
    out.push_str(&row(
        "Throughput",
        &format!(
            "read {}  write {}",
            rate(disk.read_bytes_per_sec),
            rate(disk.write_bytes_per_sec)
        ),
    ));
    out.push_str(&row(
        "Operations",
        &format!(
            "read {}/s  write {}/s",
            disk.read_ops_per_sec, disk.write_ops_per_sec
        ),
    ));
    if disk.devices.is_empty() {
        out.push_str(&row("Devices", "no devices reported"));
    } else {
        for device in &disk.devices {
            out.push_str(&row(
                &format!("Device {}", device.device),
                &format!(
                    "read {}  write {}  {} ops read/s  {} ops write/s",
                    rate(device.read_bytes_per_sec),
                    rate(device.write_bytes_per_sec),
                    device.read_ops_per_sec,
                    device.write_ops_per_sec
                ),
            ));
        }
    }
    if disk.filesystems.is_empty() {
        out.push_str(&row("Filesystems", "no filesystems reported"));
    } else {
        for filesystem in &disk.filesystems {
            let used_percent = percent(filesystem.used_bytes, filesystem.total_bytes);
            out.push_str(&row(
                &format!("Filesystem {}", filesystem.mount_point),
                &format!(
                    "{} used of {} ({used_percent:.1}%)",
                    bytes_to_human(filesystem.used_bytes),
                    bytes_to_human(filesystem.total_bytes)
                ),
            ));
        }
    }
    for note in &disk.notes {
        out.push_str(&row("Skipped", note));
    }
    out
}

/// A byte rate, in the largest unit that keeps it readable.
fn rate(bytes_per_sec: u64) -> String {
    format!("{}/s", bytes_to_human(bytes_per_sec))
}

/// One `label   value` line, with the label padded to a fixed width.
fn row(label: &str, value: &str) -> String {
    format!("  {label:<LABEL$}{value}\n", LABEL = LABEL)
}

fn percent(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 / whole as f64 * 100.0
    }
}

fn join_percent(values: &[f64]) -> String {
    if values.is_empty() {
        return "no cores reported".to_string();
    }
    values
        .iter()
        .map(|value| format!("{value:.1}%"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::metrics::CpuSample;
    use crate::core::metrics::MemorySample;

    #[test]
    fn the_health_screen_has_cpu_memory_and_swap_rows() {
        let sample = SystemSample {
            cpu: CpuSample {
                total_percent: 12.5,
                user_percent: 8.0,
                system_percent: 3.0,
                iowait_percent: 0.5,
                idle_percent: 87.5,
                per_core_percent: vec![10.0, 15.0],
                load_avg_1: 0.5,
                load_avg_5: 0.6,
                load_avg_15: 0.7,
                ..Default::default()
            },
            memory: MemorySample {
                total_bytes: 16 * 1024 * 1024 * 1024,
                used_bytes: 8 * 1024 * 1024 * 1024,
                available_bytes: 8 * 1024 * 1024 * 1024,
                cached_bytes: 1024 * 1024 * 1024,
                buffers_bytes: 512 * 1024 * 1024,
                swap_total_bytes: 2 * 1024 * 1024 * 1024,
                swap_used_bytes: 0,
            },
            ..Default::default()
        };

        let text = health(&sample);

        assert!(text.contains("CPU"), "{text}");
        assert!(text.contains("Memory"), "{text}");
        assert!(text.contains("Swap"), "{text}");
        assert!(text.contains("12.5%"), "{text}");
        assert!(text.contains("Load average"), "{text}");
        assert!(text.contains("Per-core"), "{text}");
        assert!(text.contains("10.0% 15.0%"), "{text}");
        assert!(text.contains("16.0 GiB"), "{text}");
        assert!(text.contains("2.0 GiB total, 0 B used"), "{text}");
    }

    #[test]
    fn the_disk_rows_carry_the_expected_labels() {
        let sample = SystemSample {
            disk: crate::core::metrics::DiskSample {
                read_bytes_per_sec: 2048,
                write_bytes_per_sec: 4096,
                read_ops_per_sec: 10,
                write_ops_per_sec: 20,
                devices: vec![crate::core::metrics::DiskDeviceSample {
                    device: "sda".into(),
                    read_bytes_per_sec: 2048,
                    write_bytes_per_sec: 4096,
                    read_ops_per_sec: 10,
                    write_ops_per_sec: 20,
                    latency_ms: None,
                }],
                filesystems: vec![crate::core::metrics::FilesystemSample {
                    mount_point: "/".into(),
                    used_bytes: 50,
                    total_bytes: 100,
                    available_bytes: 50,
                }],
                notes: vec!["/mnt/x: cannot read".into()],
            },
            ..Default::default()
        };

        let text = health(&sample);

        assert!(text.contains("Disk"), "{text}");
        assert!(text.contains("Throughput"), "{text}");
        assert!(text.contains("Operations"), "{text}");
        assert!(text.contains("Device sda"), "{text}");
        assert!(text.contains("Filesystem /"), "{text}");
        assert!(text.contains("50.0%"), "{text}");
        assert!(text.contains("/mnt/x: cannot read"), "{text}");
    }

    #[test]
    fn a_machine_with_no_cores_says_so() {
        let text = health(&SystemSample::default());

        assert!(text.contains("no cores reported"), "{text}");
    }
}
