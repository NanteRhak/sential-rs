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
    out
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
    fn a_machine_with_no_cores_says_so() {
        let text = health(&SystemSample::default());

        assert!(text.contains("no cores reported"), "{text}");
    }
}
