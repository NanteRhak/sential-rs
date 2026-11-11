# Rust System Sentinel

A lightweight, fast system monitor for Linux, written in Rust — htop's view of the machine, plus anomaly detection and safe resource control.

**Status: no code yet.** The design and the build plan are done; the implementation starts at ticket 01.

## Why

- The system slows down and nobody knows why.
- An app crashes unexpectedly and the culprit is hard to find.
- Resource-hungry processes need to be spotted and ranked.
- Machine health (CPU, RAM, disk I/O, network, temperature) needs one place to look.
- Some anomalies last milliseconds and slip past normal monitoring.
- On weak hardware, resources have to be rationed and critical processes protected.

## What it will do

- **System health** — CPU (total, per core, user/system/iowait/idle), load average, memory, swap, disk throughput and filesystem usage, network throughput and packet rate, temperature and sensors, kernel pressure stalls.
- **Process view** — CPU%, RSS/VSZ, per-process disk I/O, thread and file-descriptor counts, state, user, cgroup. Sortable by any resource and by a combined resource pressure score.
- **Anomaly detection** — configurable threshold rules, spike detection against moving baselines, unexpected-exit and restart-loop detection, OOM-killer evidence, and millisecond-level sampling with a ring buffer of recent high-resolution metrics.
- **Alerts** — severity, category, current value against threshold, suggested action. Shown on the terminal, written to a log file, listed in the TUI.
- **Resource control** — kill, signal, suspend, resume, renice, I/O priority, CPU affinity and cgroup limits. Dry-run by default, confirmation on destructive actions, protected processes never auto-killed, every action audited.
- **Interfaces** — a CLI for scripting, a TUI for watching, and a daemon mode for running unattended.

## Planned commands

| Command | Purpose |
| --- | --- |
| `sentinel health` | One-screen system health summary |
| `sentinel ps --sort cpu` | Process list, sortable by any resource |
| `sentinel top` | Busiest processes once, then exit |
| `sentinel tui` | Live dashboard with graphs and a process detail panel |
| `sentinel watch --interval 100ms` | High-frequency watch for short-lived spikes |
| `sentinel alerts --last 1h` | Recent alerts |
| `sentinel inspect <pid>` | Everything known about one process |
| `sentinel kill <pid>` | Safe, confirmed termination |
| `sentinel report --last 10m` | What happened in the recent past |
| `sentinel daemon` | Run in the background |

## Design constraints

- Read-only monitoring never needs root; privileges are requested only for management actions.
- Everything stays local: no telemetry, no network transmission unless explicitly configured.
- Lightweight: low CPU, bounded memory, no database server, no browser.
- Linux first, metrics read from `/proc` and `/sys`.

## Configuration

`sentinel` reads `./sentinel.toml` if it exists, otherwise
`$XDG_CONFIG_HOME/sentinel/config.toml` (`~/.config/sentinel/config.toml` by default),
otherwise built-in defaults. A `~` in a path is expanded to your home directory.

```toml
[general]
interval = "1s"
history_duration = "10m"
persist_history = true
data_dir = "~/.local/share/rust-system-sentinel"

[thresholds]
cpu_percent = 90
memory_percent = 90
temperature_celsius = 85
disk_usage_percent = 90

[critical_processes]
names = ["systemd", "sshd", "init"]
```

On weak hardware, set `profile = "low-resource"` (or `low_resource_mode = true`). It
forces an interval of at least 2 seconds, caps history at one minute, disables
persistence, and turns off the per-process I/O and network collectors.

An invalid config is refused at startup with the offending key and line number.

## Daemon mode

`sentinel daemon` collects metrics, watches the alert rules, stores the
history, writes the alerts and applies the policies in the background. It
runs until it is asked to stop, and shuts down cleanly on Ctrl-C or
`SIGTERM`. `--background` detaches it from the terminal, so it keeps going
when the terminal goes away.

Its resource budget is small on purpose:

| Budget | Default | Low-resource profile |
| --- | --- | --- |
| Sampling interval | 1 second | 2 seconds |
| CPU used | under 2% of one core | under 1% of one core |
| Memory used | under 30 MiB | under 20 MiB |
| Disk used | 10 MiB of history, then trimmed | none |

The low-resource profile turns the persistence off, so the daemon writes
nothing at all unless it is told to.

## Building

The Cargo project is created in ticket 01. Once it lands:

```bash
cargo build --release
cargo test
cargo run -- --help
```

## Documentation

- [Full specification](project/project-specs1.md) — the source of truth for scope and behaviour.
- [Problem statement](project/problem.md) and [project notes](project/about.md).
- [Implementation tickets](.scratch/system-sentinel/issues/) — 32 tracer-bullet tickets, numbered in dependency order.
