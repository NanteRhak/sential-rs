# Rust System Sentinel

A lightweight, fast system monitor for Linux, written in Rust — htop's view of the machine, plus anomaly detection and safe resource control.

**Status: implemented.** The tool builds, the tests pass, and every ticket in
the plan has landed.

## Why

- The system slows down and nobody knows why.
- An app crashes unexpectedly and the culprit is hard to find.
- Resource-hungry processes need to be spotted and ranked.
- Machine health (CPU, RAM, disk I/O, network, temperature) needs one place to look.
- Some anomalies last milliseconds and slip past normal monitoring.
- On weak hardware, resources have to be rationed and critical processes protected.

## What it does

- **System health** — CPU (total, per core, user/system/iowait/idle), load average, memory, swap, disk throughput and filesystem usage, network throughput and packet rate, temperature and sensors, kernel pressure stalls.
- **Process view** — CPU%, RSS/VSZ, per-process disk I/O, thread and file-descriptor counts, state, user, cgroup. Sortable by any resource and by a combined resource pressure score.
- **Anomaly detection** — configurable threshold rules, spike detection against moving baselines, unexpected-exit and restart-loop detection, OOM-killer evidence, and millisecond-level sampling with a ring buffer of recent high-resolution metrics.
- **Alerts** — severity, category, current value against threshold, suggested action. Shown on the terminal, written to a log file, listed in the TUI.
- **Resource control** — kill, signal, suspend, resume, renice, I/O priority, CPU affinity and cgroup limits. Dry-run by default, confirmation on destructive actions, protected processes never auto-killed, every action audited.
- **Interfaces** — a CLI for scripting, a TUI for watching, and a daemon mode for running unattended.

## Commands

| Command | Purpose |
| --- | --- |
| `sentinel health` | One-screen system health summary |
| `sentinel ps --sort cpu` | Process list, sortable by any resource |
| `sentinel ps --json` | The same list, for scripting |
| `sentinel top` | Busiest processes once, then exit |
| `sentinel tui` | Live dashboard with graphs and a process detail panel |
| `sentinel watch --interval 100ms` | High-frequency watch for short-lived spikes |
| `sentinel alerts --last 1h` | Recent alerts |
| `sentinel inspect <pid>` | Everything known about one process |
| `sentinel kill <pid>` | Safe, confirmed termination |
| `sentinel suspend <pid>` / `sentinel resume <pid>` | Stop and continue a process |
| `sentinel renice <pid> 5` | Change a process priority |
| `sentinel limit <pid> --cpu 50` | Cap what a process may use, with a cgroup |
| `sentinel report --last 10m` | What happened in the recent past |
| `sentinel daemon` | Run in the background |

Every action that touches a process asks first, unless `--yes` is given, and
`--dry-run` reports what would happen without doing it.

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

[actions]
log_file = "~/.local/state/sentinel/actions.log"

[[alerts.rules]]
name = "high-cpu"
metric = "cpu_percent"
condition = "above"
threshold = 90
duration = "5s"
severity = "warning"
message = "CPU has been busy for five seconds"

[[policies.rules]]
name = "kill-hog"
enabled = false
metric = "process_cpu_percent"
condition = "above"
threshold = 95
duration = "30s"
action = "kill"
cooldown = "10m"
exclude = ["sshd"]
```

A rule may also be a spike, measured against a moving baseline:

```toml
[[alerts.rules]]
name = "cpu-spike"
metric = "cpu_percent"
condition = "spike"
severity = "warning"
message = "CPU spiked"

[alerts.rules.spike]
baseline = "moving-average"   # or "ewma"
window = "30s"
multiple = 3.0
minimum = 50.0
```

The other sections are `[ui]`, `[sampling]`, `[thresholds]`, `[detection]`,
`[pressure]` (the weights of the pressure score), `[history]` (how much
history is kept on disk) and `[daemon]` (where the daemon logs).

On weak hardware, set `profile = "low-resource"` (or `low_resource_mode = true`). It
forces an interval of at least 2 seconds, caps history at one minute, disables
persistence, and turns off the per-process I/O and network collectors.

An invalid config is refused at startup with the offending key and line number.

## Installation

From a release tarball:

```bash
make package                       # builds build/sentinel-<version>.tar.gz
tar -C build -xzf build/sentinel-*.tar.gz
sudo make install                  # into /usr/local
```

Or with Cargo:

```bash
cargo install --path .
```

`make install` puts the binary in `/usr/local/bin` and the service unit in
`/usr/local/lib/systemd/system`. The configuration file is looked for in
this order:

1. the file named by the `SENTINEL_CONFIG` environment variable
2. `sentinel.toml` in the current directory
3. `$XDG_CONFIG_HOME/sentinel/config.toml`, or
   `~/.config/sentinel/config.toml`

## Running it as a service

The service unit is `packaging/systemd/sentinel.service`. It runs the daemon
as an unprivileged `sentinel` user, restarts it when it fails, and writes
what the daemon is doing to `/var/lib/sentinel/sentinel.log`.

```bash
sudo useradd --system --no-create-home sentinel
sudo install -d -o sentinel -g sentinel /etc/sentinel /var/lib/sentinel
sudo install -Dm644 packaging/systemd/sentinel.service \
    /etc/systemd/system/sentinel.service
sudo systemctl daemon-reload
sudo systemctl enable --now sentinel
```

The configuration of the service lives in `/etc/sentinel/config.toml`, which
the unit points at with `SENTINEL_CONFIG`. The unit creates
`/etc/sentinel` and `/var/lib/sentinel` itself with `ConfigurationDirectory`
and `StateDirectory`.

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

```bash
cargo build --release
cargo test
cargo run -- --help
```

`make lint` runs the formatter check and clippy; `make test` runs the tests.

## Documentation

- [Full specification](project/project-specs1.md) — the source of truth for scope and behaviour.
- [Problem statement](project/problem.md) and [project notes](project/about.md).
- [Implementation tickets](.scratch/system-sentinel/issues/) — the 32 tracer-bullet tickets, numbered in dependency order, all landed.
