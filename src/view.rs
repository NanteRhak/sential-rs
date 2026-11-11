//! Text views: the one-screen summaries the CLI prints.

use crate::collect::process::state_letter;
use crate::core::format::bytes_to_human;
use crate::core::metrics::SystemSample;
use crate::core::process::ProcessSample;

const LABEL: usize = 18;

/// The `sentinel health` screen.
pub fn health(sample: &SystemSample, show_network: bool, show_temperature: bool) -> String {
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
    if show_network {
        out.push('\n');
        out.push_str(&network_section(sample));
    }
    if show_temperature {
        out.push('\n');
        out.push_str(&sensor_section(sample));
    }
    out
}

/// The hardware-health row: temperatures, fans, throttling and pressure.
fn sensor_section(sample: &SystemSample) -> String {
    let sensors = &sample.sensors;
    let mut out = String::new();
    out.push_str("Sensors\n");
    if sensors.temperatures.is_empty() {
        out.push_str(&row("Temperature", "no readable sensors"));
    } else {
        let readings = sensors
            .temperatures
            .iter()
            .map(|reading| format!("{} {:.1}°C", reading.label, reading.celsius))
            .collect::<Vec<_>>()
            .join("  ");
        out.push_str(&row("Temperature", &readings));
    }
    if sensors.fans.is_empty() {
        out.push_str(&row("Fans", "no readable sensors"));
    } else {
        let readings = sensors
            .fans
            .iter()
            .map(|reading| format!("{} {} rpm", reading.label, reading.rpm))
            .collect::<Vec<_>>()
            .join("  ");
        out.push_str(&row("Fans", &readings));
    }
    out.push_str(&row(
        "Throttling",
        match sensors.throttled {
            Some(true) => "yes, the processor is throttled",
            Some(false) => "no",
            None => "not reported",
        },
    ));
    if let Some(profile) = &sensors.power_profile {
        out.push_str(&row("Power profile", profile));
    }
    if let Some(pressure) = &sensors.pressure {
        out.push_str(&row(
            "Pressure stall",
            &format!(
                "cpu {:.1}%  memory {:.1}%  io {:.1}%",
                pressure.cpu_some_avg10, pressure.memory_some_avg10, pressure.io_some_avg10
            ),
        ));
    }
    out
}

/// The network row: throughput, packet rate, errors and drops.
fn network_section(sample: &SystemSample) -> String {
    let network = &sample.network;
    let mut out = String::new();
    out.push_str("Network\n");
    out.push_str(&row(
        "Throughput",
        &format!(
            "received {}  sent {}",
            rate(network.rx_bytes_per_sec),
            rate(network.tx_bytes_per_sec)
        ),
    ));
    out.push_str(&row(
        "Packets",
        &format!(
            "{}/s received  {}/s sent",
            network.rx_packets_per_sec, network.tx_packets_per_sec
        ),
    ));
    out.push_str(&row(
        "Errors",
        &format!("{}  drops {}", network.errors_total, network.drops_total),
    ));
    if network.interfaces.is_empty() {
        out.push_str(&row("Interfaces", "no interfaces reported"));
    } else {
        for interface in &network.interfaces {
            out.push_str(&row(
                &format!("Interface {}", interface.name),
                &format!(
                    "received {}  sent {}  {}/s in  {}/s out  errors {}  drops {}",
                    rate(interface.rx_bytes_per_sec),
                    rate(interface.tx_bytes_per_sec),
                    interface.rx_packets_per_sec,
                    interface.tx_packets_per_sec,
                    interface.errors_total,
                    interface.drops_total
                ),
            ));
        }
    }
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

/// The column the process list is ordered by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    #[default]
    Cpu,
    Mem,
    Name,
    Io,
    Net,
    Pressure,
}

impl Sort {
    pub fn of(key: crate::cli::SortKey) -> Sort {
        match key {
            crate::cli::SortKey::Cpu => Sort::Cpu,
            crate::cli::SortKey::Mem => Sort::Mem,
            crate::cli::SortKey::Name => Sort::Name,
            crate::cli::SortKey::Pressure => Sort::Pressure,
        }
    }
}

/// Order the rows by the chosen column. Ties keep their original order.
pub fn sort_processes(processes: &mut [ProcessSample], sort: Sort) {
    match sort {
        Sort::Cpu => processes.sort_by(|a, b| {
            b.cpu_percent
                .partial_cmp(&a.cpu_percent)
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        Sort::Mem => processes.sort_by(|a, b| {
            b.mem_rss_bytes
                .cmp(&a.mem_rss_bytes)
                .then_with(|| a.pid.cmp(&b.pid))
        }),
        Sort::Name => processes.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.pid.cmp(&b.pid))),
        Sort::Io => processes.sort_by(|a, b| {
            io_bytes(b)
                .cmp(&io_bytes(a))
                .then_with(|| a.pid.cmp(&b.pid))
        }),
        Sort::Net => processes.sort_by(|a, b| {
            net_bytes(b)
                .cmp(&net_bytes(a))
                .then_with(|| a.pid.cmp(&b.pid))
        }),
        Sort::Pressure => processes.sort_by(|a, b| {
            b.pressure_score
                .partial_cmp(&a.pressure_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.pid.cmp(&b.pid))
        }),
    }
}

/// The bytes a process reads and writes in a second.
pub fn io_bytes(process: &ProcessSample) -> u64 {
    process.read_bytes_per_sec + process.write_bytes_per_sec
}

/// The bytes a process sends and receives in a second.
pub fn net_bytes(process: &ProcessSample) -> u64 {
    process.net_rx_bytes_per_sec.unwrap_or(0) + process.net_tx_bytes_per_sec.unwrap_or(0)
}

/// An exit event, as it is shown.
pub fn exit_line(exit: &crate::exits::ExitRecord) -> String {
    format!(
        "{} ({}) after {}: {}",
        exit.process_name,
        exit.pid,
        crate::view::uptime(exit.last_uptime_secs),
        exit.reason()
    )
}

/// A proposed action, as it is shown.
pub fn proposal_line(proposal: &crate::policy::Proposal) -> String {
    format!(
        "{} would {} {} ({}): {}",
        proposal.policy,
        proposal.action.label(),
        proposal.process_name,
        proposal.pid,
        proposal.reason
    )
}

/// One line of the watch view.
pub fn watch_line(sample: &SystemSample) -> String {
    let memory = &sample.memory;
    let memory_percent = percent(memory.used_bytes, memory.total_bytes);
    format!(
        "[{}] cpu {:>5.1}%  mem {:>5.1}%  disk read {} write {}  net received {} sent {}  load {:.2}",
        clock(sample.timestamp / 1_000),
        sample.cpu.total_percent,
        memory_percent,
        rate(sample.disk.read_bytes_per_sec),
        rate(sample.disk.write_bytes_per_sec),
        rate(sample.network.rx_bytes_per_sec),
        rate(sample.network.tx_bytes_per_sec),
        sample.cpu.load_avg_1,
    )
}

/// An alert, as it is shown on the terminal.
pub fn alert_line(alert: &crate::alert::Alert) -> String {
    let severity = match alert.severity {
        crate::core::Severity::Info => "INFO",
        crate::core::Severity::Warning => "WARNING",
        crate::core::Severity::Critical => "CRITICAL",
    };
    let value = match alert.threshold {
        Some(threshold) => format!(
            "{} ({:.1} vs {:.1})",
            alert.metric.clone().unwrap_or_default(),
            alert.current_value,
            threshold
        ),
        None => format!(
            "{} ({:.1})",
            alert.metric.clone().unwrap_or_default(),
            alert.current_value
        ),
    };
    let process = match (&alert.process_name, alert.pid) {
        (Some(name), Some(pid)) => format!(" [{name} {pid}]"),
        _ => String::new(),
    };
    format!(
        "[{severity}] {}{process}: {value} — {}",
        alert.message, alert.suggestion
    )
}

/// Everything known about one process.
pub fn process_detail(process: &ProcessSample) -> String {
    let mut out = String::new();
    out.push_str(&row(
        "Process",
        &format!("{} ({})", process.name, process.pid),
    ));
    out.push_str(&row(
        "Command",
        if process.cmd.is_empty() {
            "not readable"
        } else {
            &process.cmd
        },
    ));
    out.push_str(&row("Parent", &process.ppid.to_string()));
    out.push_str(&row("User", &process.user));
    out.push_str(&row(
        "Started",
        &format!(
            "{} (up {})",
            clock(process.started_at),
            uptime(process.uptime_secs)
        ),
    ));
    out.push_str(&row(
        "Usage",
        &format!(
            "cpu {:.1}%  memory {} ({:.1}%)",
            process.cpu_percent,
            bytes_to_human(process.mem_rss_bytes),
            process.mem_percent
        ),
    ));
    out.push_str(&row("Threads", &process.thread_count.to_string()));
    out.push_str(&row("File descriptors", &process.fd_count.to_string()));
    out.push_str(&row("Nice", &process.nice.to_string()));
    out.push_str(&row(
        "Cgroup",
        process.cgroup.as_deref().unwrap_or("not readable"),
    ));
    out
}

/// The time of day, in UTC.
pub fn clock(epoch_secs: u64) -> String {
    let seconds = epoch_secs % 86_400;
    format!(
        "{:02}:{:02}:{:02} UTC",
        seconds / 3_600,
        (seconds % 3_600) / 60,
        seconds % 60
    )
}

/// The process list as JSON, for scripting.
pub fn processes_json(processes: &[ProcessSample]) -> String {
    let value = serde_json::json!({ "processes": processes });
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string())
}

/// The `sentinel ps` table.
pub fn processes(processes: &[ProcessSample]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{:>7}  {:>7}  {:<10}  {:<5}  {:>6}  {:>9}  {:>10}  {:>10}  {:>6}  {:>6}  {}\n",
        "PID", "PPID", "USER", "STATE", "CPU%", "ELAPSED", "RSS", "VSZ", "MEM%", "PRS", "NAME"
    ));
    for process in processes {
        out.push_str(&format!(
            "{:>7}  {:>7}  {:<10}  {:<5}  {:>5.1}  {:>9}  {:>10}  {:>10}  {:>5.1}%  {:>5.1}  {}\n",
            process.pid,
            process.ppid,
            process.user,
            state_letter(process.state),
            process.cpu_percent,
            uptime(process.uptime_secs),
            bytes_to_human(process.mem_rss_bytes),
            bytes_to_human(process.mem_vsz_bytes),
            process.mem_percent,
            process.pressure_score,
            process.name,
        ));
    }
    out
}

/// How long a process has been running, in the units a person expects.
pub fn uptime(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let seconds = seconds % 60;
    if days > 0 {
        format!("{days}d {hours:02}:{minutes:02}")
    } else if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
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

        let text = health(&sample, true, true);

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

        let text = health(&sample, true, true);

        assert!(text.contains("Disk"), "{text}");
        assert!(text.contains("Throughput"), "{text}");
        assert!(text.contains("Operations"), "{text}");
        assert!(text.contains("Device sda"), "{text}");
        assert!(text.contains("Filesystem /"), "{text}");
        assert!(text.contains("50.0%"), "{text}");
        assert!(text.contains("/mnt/x: cannot read"), "{text}");
    }

    #[test]
    fn the_network_rows_carry_the_expected_labels() {
        let sample = SystemSample {
            network: crate::core::metrics::NetworkSample {
                rx_bytes_per_sec: 1024,
                tx_bytes_per_sec: 2048,
                rx_packets_per_sec: 10,
                tx_packets_per_sec: 20,
                errors_total: 3,
                drops_total: 4,
                interfaces: vec![crate::core::metrics::InterfaceSample {
                    name: "eth0".into(),
                    rx_bytes_per_sec: 1024,
                    tx_bytes_per_sec: 2048,
                    rx_packets_per_sec: 10,
                    tx_packets_per_sec: 20,
                    errors_total: 3,
                    drops_total: 4,
                }],
            },
            ..Default::default()
        };

        let text = health(&sample, true, true);

        assert!(text.contains("Network"), "{text}");
        assert!(text.contains("Throughput"), "{text}");
        assert!(text.contains("Packets"), "{text}");
        assert!(text.contains("Errors"), "{text}");
        assert!(text.contains("Interface eth0"), "{text}");
        assert!(text.contains("10/s received"), "{text}");
    }

    #[test]
    fn the_network_row_can_be_left_out() {
        let text = health(&SystemSample::default(), false, true);

        assert!(!text.contains("Network"), "{text}");
    }

    #[test]
    fn the_process_table_has_a_header_and_a_row() {
        let sample = ProcessSample {
            pid: 42,
            ppid: 1,
            name: "sleep".into(),
            user: "root".into(),
            state: crate::core::process::ProcessState::Sleeping,
            mem_rss_bytes: 1024 * 1024,
            mem_vsz_bytes: 4 * 1024 * 1024,
            mem_percent: 1.5,
            uptime_secs: 3_661,
            ..Default::default()
        };

        let text = processes(&[sample]);

        assert!(text.contains("PID"), "{text}");
        assert!(text.contains("MEM%"), "{text}");
        assert!(text.contains("sleep"), "{text}");
        assert!(text.contains("1.0 MiB"), "{text}");
        assert!(text.contains("1:01:01"), "{text}");
    }

    fn sample(pid: u32, name: &str, cpu: f64, rss: u64) -> ProcessSample {
        ProcessSample {
            pid,
            name: name.into(),
            cpu_percent: cpu,
            mem_rss_bytes: rss,
            ..Default::default()
        }
    }

    #[test]
    fn sorting_by_memory_puts_the_biggest_first() {
        let mut samples = vec![
            sample(1, "a", 1.0, 100),
            sample(2, "b", 2.0, 500),
            sample(3, "c", 3.0, 300),
        ];

        sort_processes(&mut samples, Sort::Mem);

        assert_eq!(samples.iter().map(|s| s.pid).collect::<Vec<_>>(), [2, 3, 1]);
    }

    #[test]
    fn sorting_by_cpu_puts_the_busiest_first() {
        let mut samples = vec![
            sample(1, "a", 1.0, 100),
            sample(2, "b", 5.0, 500),
            sample(3, "c", 3.0, 300),
        ];

        sort_processes(&mut samples, Sort::Cpu);

        assert_eq!(samples.iter().map(|s| s.pid).collect::<Vec<_>>(), [2, 3, 1]);
    }

    #[test]
    fn sorting_by_name_is_alphabetical() {
        let mut samples = vec![
            sample(1, "zebra", 1.0, 100),
            sample(2, "apple", 2.0, 500),
            sample(3, "mango", 3.0, 300),
        ];

        sort_processes(&mut samples, Sort::Name);

        assert_eq!(
            samples.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            ["apple", "mango", "zebra"]
        );
    }

    #[test]
    fn the_json_output_parses() {
        let samples = vec![sample(1, "a", 1.5, 100)];

        let text = processes_json(&samples);
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();

        let rows = value["processes"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["pid"], 1);
        assert_eq!(rows[0]["name"], "a");
        assert_eq!(rows[0]["cpu_percent"], 1.5);
    }

    #[test]
    fn uptime_is_shown_in_the_units_a_person_expects() {
        assert_eq!(uptime(30), "00:30");
        assert_eq!(uptime(3_661), "1:01:01");
        assert_eq!(uptime(90_061), "1d 01:01");
    }

    #[test]
    fn an_alert_line_shows_the_severity_and_the_numbers() {
        let alert = crate::alert::Alert {
            id: 1,
            timestamp: 0,
            severity: crate::core::Severity::Critical,
            category: crate::alert::AlertCategory::Cpu,
            message: "CPU has been busy".into(),
            metric: Some("cpu_percent".into()),
            pid: Some(42),
            process_name: Some("backup".into()),
            current_value: 95.0,
            threshold: Some(90.0),
            baseline: None,
            suggestion: "look at the busiest processes".into(),
        };

        let line = alert_line(&alert);

        assert!(line.contains("[CRITICAL]"), "{line}");
        assert!(line.contains("CPU has been busy"), "{line}");
        assert!(line.contains("backup 42"), "{line}");
        assert!(line.contains("95.0"), "{line}");
        assert!(line.contains("90.0"), "{line}");
    }

    #[test]
    fn a_machine_with_no_sensors_says_so_instead_of_failing() {
        let text = health(&SystemSample::default(), true, true);

        assert!(text.contains("Sensors"), "{text}");
        assert!(text.contains("no readable sensors"), "{text}");
        assert!(text.contains("not reported"), "{text}");
    }

    #[test]
    fn a_machine_with_no_cores_says_so() {
        let text = health(&SystemSample::default(), true, true);

        assert!(text.contains("no cores reported"), "{text}");
    }
}
