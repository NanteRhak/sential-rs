//! Renders the dashboard as text, so the README can show what it looks like.
//!
//! ```text
//! cargo run --example dashboard
//! ```

use std::sync::Arc;

use sentinel::core::metrics::{
    CpuSample, DiskDeviceSample, DiskSample, FilesystemSample, InterfaceSample, MemorySample,
    NetworkSample, SensorSample, SystemSample, TemperatureSample,
};
use sentinel::core::process::{ProcessSample, ProcessState};
use sentinel::sample::{SamplerState, Shared};
use sentinel::tui::app::App;

fn main() {
    let shared = Arc::new(Shared::default());
    // The graphs need a few samples before they have something to draw.
    for tick in 0..40 {
        let mut system = system();
        system.cpu.total_percent = 8.0 + (tick % 12) as f64 * 1.7;
        system.memory.used_bytes = 7_900_000_000 + (tick % 7) as u64 * 90_000_000;
        shared.publish(
            Arc::new(SamplerState {
                tick: tick as u64,
                system,
                processes: processes(),
                alerts: Vec::new(),
                exits: Vec::new(),
                restart_loops: Vec::new(),
                proposals: Vec::new(),
            }),
            std::time::Duration::from_secs(600),
        );
    }

    let app = App::new(shared);
    print!("{}", sentinel::tui::app::render(&app, 104, 34));
}

fn system() -> SystemSample {
    SystemSample {
        timestamp: 0,
        cpu: CpuSample {
            total_percent: 18.4,
            per_core_percent: vec![12.0, 31.5, 9.4, 22.7],
            user_percent: 9.1,
            system_percent: 6.2,
            iowait_percent: 0.4,
            idle_percent: 81.6,
            load_avg_1: 1.42,
            load_avg_5: 1.10,
            load_avg_15: 0.87,
            ..Default::default()
        },
        memory: MemorySample {
            total_bytes: 15_933_685_504,
            used_bytes: 8_589_934_592,
            available_bytes: 7_343_750_912,
            cached_bytes: 3_500_000_000,
            buffers_bytes: 91_000_000,
            swap_total_bytes: 8_000_000_000,
            swap_used_bytes: 1_100_000_000,
        },
        disk: DiskSample {
            read_bytes_per_sec: 1_048_576,
            write_bytes_per_sec: 12_582_912,
            read_ops_per_sec: 42,
            write_ops_per_sec: 310,
            devices: vec![DiskDeviceSample {
                device: "nvme0n1".into(),
                read_bytes_per_sec: 1_048_576,
                write_bytes_per_sec: 12_582_912,
                read_ops_per_sec: 42,
                write_ops_per_sec: 310,
                latency_ms: Some(0.8),
            }],
            filesystems: vec![FilesystemSample {
                mount_point: "/".into(),
                used_bytes: 145_000_000_000,
                total_bytes: 181_000_000_000,
                available_bytes: 36_000_000_000,
            }],
            notes: Vec::new(),
        },
        network: NetworkSample {
            rx_bytes_per_sec: 524_288,
            tx_bytes_per_sec: 131_072,
            rx_packets_per_sec: 410,
            tx_packets_per_sec: 220,
            errors_total: 0,
            drops_total: 0,
            interfaces: vec![InterfaceSample {
                name: "wlp2s0".into(),
                rx_bytes_per_sec: 524_288,
                tx_bytes_per_sec: 131_072,
                rx_packets_per_sec: 410,
                tx_packets_per_sec: 220,
                errors_total: 0,
                drops_total: 0,
            }],
        },
        sensors: SensorSample {
            temperatures: vec![TemperatureSample {
                label: "x86_pkg_temp".into(),
                celsius: 57.0,
            }],
            ..Default::default()
        },
    }
}

fn processes() -> Vec<ProcessSample> {
    let rows = [
        (1, "systemd", "root", 0.2, 12.4, 3_661),
        (2, "firefox", "you", 18.4, 612.0, 14_402),
        (3, "backup-agent", "you", 9.1, 1_024.0, 502),
        (4, "postgres", "postgres", 2.2, 256.0, 8_921),
        (5, "rustc", "you", 44.7, 512.0, 12),
    ];
    rows.into_iter()
        .map(|(pid, name, user, cpu, rss, uptime)| ProcessSample {
            timestamp: 0,
            pid,
            start_time: 1000 + pid as u64,
            name: name.into(),
            cmd: format!("/usr/bin/{name}"),
            user: user.into(),
            cpu_percent: cpu,
            mem_rss_bytes: (rss * 1024.0 * 1024.0) as u64,
            mem_vsz_bytes: (rss * 4.0 * 1024.0 * 1024.0) as u64,
            mem_percent: rss / 16.0,
            read_bytes_per_sec: 0,
            write_bytes_per_sec: 0,
            net_rx_bytes_per_sec: None,
            net_tx_bytes_per_sec: None,
            thread_count: 8,
            fd_count: 64,
            state: ProcessState::Running,
            nice: 0,
            pressure_score: cpu / 10.0,
            uptime_secs: uptime,
            ppid: 1,
            started_at: 0,
            cgroup: Some("/user.slice".into()),
            net_sockets: 0,
        })
        .collect()
}
