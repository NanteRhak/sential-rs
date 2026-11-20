# Rust System Sentinel

**A lightweight Linux system monitor with anomaly detection and safe resource control.**
htop's view of the machine, plus the things htop cannot tell you: what happened
ten seconds ago, which process spiked for 200 ms, and what to do about it.

[![CI](https://github.com/NanteRhak/sential-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/NanteRhak/sential-rs/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/rust-2024--orange?style=flat-square)](https://www.rust-lang.org/)
[![Linux](https://img.shields.io/badge/platform-linux-lightgrey?style=flat-square)](https://www.kernel.org/)
[![Dependencies](https://img.shields.io/badge/dependencies-7-informational?style=flat-square)](Cargo.toml)

```text
╭ CPU ──────────────────────────────────────────────────╮╭ Memory ─────────────────────────────────────╮
│CPU       18.4%  user 9.1%  sys 6.2%  iowait 0.4%      ││Memory    8.0 GiB used / 14.8 GiB            │
│Load      1.42 1.10 0.87   x86_pkg_temp 57.0°C         ││Used      ██████░░░░░░ 54%  free 6.8 GiB     │
│ 0 ███░░░░░░░░░░░░░░░░░░░░░  12%                       ││Cached    3.3 GiB   Buffers 86.8 MiB         │
│ 1 ████████░░░░░░░░░░░░░░░░  32%                       ││Swap      1.0 GiB used / 7.5 GiB             │
│ 2 ██░░░░░░░░░░░░░░░░░░░░░░   9%                       │╰─────────────────────────────────────────────╯
│ 3 █████░░░░░░░░░░░░░░░░░░░  23%                       │╭ Network ────────────────────────────────────╮
│                                                       ││Network   received 512.0 KiB  sent 128.0 KiB │
│                                                       ││wlp2s0    received 512.0 KiB  sent 128.0 KiB │
│                                                       ││                                             │
╰───────────────────────────────────────────────────────╯╰─────────────────────────────────────────────╯
╭ Disk ────────────────────────────────────────────────────────────────────────────────────────────────╮
│Disk      read 1.0 MiB  write 12.0 MiB                                                                │
│Devices   nvme0n1 r 1.0 MiB w 12.0 MiB                                                                │
│Mounts    / 80% of 168.6 GiB                                                                          │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────╯
╭ Alerts ──────────────────────────────────────────────────────────────────────────────────────────────╮
│no alerts                                                                                             │
│                                                                                                      │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────╯
╭ Processes (5) | sort: cpu ───────────────────────────────────────────────────────────────────────────╮
│PID      USER         CPU%    MEM%    NAME                                                            │
│5        you          44.7    32.0    rustc                                                           │
│2        you          18.4    38.2    firefox                                                         │
│3        you          9.1     64.0    backup-agent                                                    │
│4        postgres     2.2     16.0    postgres                                                        │
│1        root         0.2     0.8     systemd                                                         │
│                                                                                                      │
│                                                                                                      │
│                                                                                                      │
│                                                                                                      │
│                                                                                                      │
│                                                                                                      │
│                                                                                                      │
│                                                                                                      │
│                                                                                                      │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────╯
 q quit   t/m/i/N sort   f filter   / search   ↑↓ select   enter detail   a actions                     
```

<sub>The picture above is rendered by `cargo run --example dashboard`, so it is the
real dashboard and not a drawing of one.</sub>

- [Why](#why)
- [What it does](#what-it-does)
- [Install](#install)
- [Use it](#use-it)
- [Commands](#commands)
- [The dashboard](#the-dashboard)
- [Configuration](#configuration)
- [Alerts](#alerts)
- [Policies](#policies)
- [Running it as a service](#running-it-as-a-service)
- [How it works](#how-it-works)
- [Development](#development)
- [Project structure](#project-structure)
- [Known limits](#known-limits)
- [License](#license)

## Why

- The system slows down and nobody knows why.
- An app crashes unexpectedly and the culprit is hard to find.
- Some anomalies last milliseconds and slip past a monitor that samples once a
  second.
- On weak hardware, resources have to be rationed and critical processes kept
  alive.

## What it does

- **System health** — CPU (total, per core, user/system/iowait/idle), load
  average, memory, swap, disk throughput and filesystem usage, network
  throughput and packet rate, temperatures, fans, throttling, kernel pressure
  stalls.
- **Process view** — CPU%, RSS/VSZ, per-process disk I/O, thread and
  file-descriptor counts, state, user, cgroup, and one **pressure score** that
  combines everything into a single number.
- **Anomaly detection** — threshold rules, spike detection against moving
  baselines, unexpected-exit detection, restart-loop detection and
  OOM-killer evidence.
- **High-resolution sampling** — down to 100 ms, with a bounded ring buffer so
  a 200 ms spike is still there when you go looking for it.
- **Alerts** — severity, category, value against threshold, suggested action.
  Shown on the terminal, written to a log file, listed in the dashboard.
- **Resource control** — kill, signal, suspend, resume, renice and cgroup
  limits. Every action asks first, protected processes are never killed
  automatically, and every action is written down.
- **Three interfaces** — a CLI for scripting, a live dashboard, and a daemon
  for running unattended.

Everything is read from `/proc` and `/sys`. Nothing leaves the machine, and
nothing is sent anywhere.

## Install

From source:

```bash
git clone git@github.com:NanteRhak/sential-rs.git
cd sential-rs
cargo install --path .
```

From a release tarball:

```bash
make package                       # builds build/sentinel-<version>.tar.gz
tar -C build -xzf build/sentinel-*.tar.gz
sudo make install                  # into /usr/local
```

`make install` puts the binary in `/usr/local/bin` and the service unit in
`/usr/local/lib/systemd/system`. There is no runtime dependency beyond libc:
`clap`, `serde`, `toml`, `ratatui`, `crossterm` and `serde_json` are compiled in.

## Use it

A one-screen summary of the machine:

```console
$ sentinel health
CPU
  Total             32.1%  user 28.3%  system 3.8%  iowait 0.0%  idle 67.9%
  Per-core          7.7% 7.1% 88.5% 11.5%
  Load average      0.70 0.61 0.65
  Context switches  152307281  interrupts 92848853

Memory
  Total             7.6 GiB
  Used              3.6 GiB (47.5%)
  Available         4.0 GiB
  Cached            3.6 GiB
  Buffers           88.9 MiB
  Swap              7.6 GiB total, 1.0 GiB used

Disk
  Throughput        read 0 B/s  write 0 B/s
  Operations        read 0/s  write 0/s
  Device sda        read 0 B/s  write 0 B/s  0 ops read/s  0 ops write/s
  Filesystem /      136.5 GiB used of 168.6 GiB (81.0%)
  Filesystem /boot/efi24.6 MiB used of 511.0 MiB (4.8%)
  Filesystem /run/media/you/Backup1567.6 GiB used of 710.8 GiB (79.8%)

Network
  Throughput        received 0 B/s  sent 431 B/s
  Packets           0/s received  3/s sent
  Errors            0  drops 0
  Interface eth0    received 0 B/s  sent 0 B/s  0/s in  0/s out  errors 0  drops 0
  Interface wlan0   received 0 B/s  sent 431 B/s  0/s in  3/s out  errors 0  drops 0

Sensors
  Temperature       acpi 52.0°C  cpu_package 60.0°C  hwmon1 temp1 52.0°C  hwmon1 temp2 52.0°C  Package id 0 64.0°C  Core 0 63.0°C  Core 1 63.0°C
  Fans              no readable sensors
  Throttling        no
  Pressure stall    cpu 0.4%  memory 0.0%  io 1.0%
```

The processes, ordered by the resource you care about:

```console
$ sentinel ps --sort pressure
    PID     PPID  USER        STATE    CPU%    ELAPSED         RSS         VSZ    MEM%     PRS  NAME
  92063        1  you         S        0.9    4:49:27   599.8 MiB    20.4 GiB    7.7%    3.1  firefox
  98328    97033  you         S        4.7    2:53:55   653.7 MiB    13.3 GiB    8.4%    2.1  opencode
 243358   243347  you         R        4.7      00:01     4.6 MiB     5.8 MiB    0.1%    1.0  sentinel
```

The same list, for a script:

```console
$ sentinel ps --json | jq -r '.processes[] | select(.cpu_percent > 0) | .name' | head -3
firefox
Isolated Web Co
sentinel
```

What happened in the last ten minutes:

```console
$ sentinel report --last 10m
Report for the last 10:00 (11 samples)
  CPU               peak 35.0%  average 25.3%
  Memory            peak 47.6%  average 47.5%
  Load              peak 0.54
  Disk              read peak 0 B  write peak 0 B
  Network           received peak 2.9 KiB  sent peak 1.0 KiB
  Process           Isolated Web Co (96445) cpu 63.2%  memory 9.1%
  Process           firefox (92063) cpu 50.8%  memory 7.7%
  Process           sentinel (245393) cpu 26.0%  memory 0.1%
```

And a runaway process, stopped safely:

```console
$ sentinel kill 12345
Kill backup-agent (12345)? [y/N] y
sent SIGTERM to backup-agent (12345)
```

## Commands

The command line is organized around four things you might want to do. Every
command also accepts `--help`, which lists all of its options.

### Look at the machine

`sentinel health` prints everything the machine is doing right now — CPU,
memory, disks, network, temperatures — in one screen (see the example above).
`sentinel top` is its process-focused cousin: the busiest processes, printed
once, then it exits.

```console
$ sentinel top
```

If you want the process list with more columns, use `sentinel ps`. Sort it by
CPU, memory, name or by the **pressure score** — a single number that combines
how much CPU, memory, disk, network and file descriptors a process is using:

```console
$ sentinel ps --sort mem --json
```

Add `--json` to any of these and you get machine-readable output, ready to
pipe into `jq` or another script.

### Watch for problems

`sentinel watch` samples the machine many times a second. Use it when
something happens for a moment and is gone before a slower tool can see it —
a CPU spike, a burst of disk writes. In this example, it samples five times a
second for ten seconds and prints the peaks:

```console
$ sentinel watch --interval 200ms --duration 10s
```

`sentinel report` looks backwards instead: it reads the history on disk and
summarizes the recent past — peaks, averages, and which processes were the
hungriest (see the example above). `sentinel alerts` lists the alerts that
have been raised, read back from the alert log:

```console
$ sentinel alerts --last 1h
```

### Deal with a runaway process

Start with `sentinel inspect <pid>`: it prints everything known about one
process — who started it, how much of everything it is using, how long it has
been running, and its cgroup.

```console
$ sentinel inspect 12345
```

Then decide. `sentinel kill` stops the process, but not carelessly: it asks
you first, it sends the polite termination signal (SIGTERM) so the process
can clean up after itself, and it writes the action to the action log. If you
want to see what would happen without doing it, use `--dry-run`:

```console
$ sentinel kill 12345 --dry-run
```

Other options, same safety rules:

- `sentinel suspend <pid>` — pause the process without ending it, and
  `sentinel resume <pid>` to continue it later.
- `sentinel renice <pid> <value>` — make the process lower or higher priority,
  so it bothers the rest of the machine less without being killed.
- `sentinel limit <pid> --cpu 50 --memory 20` — put the process under a cgroup
  so it may use at most 50% of one CPU core and 20% of memory. Remove the cap
  with `--remove`. (Needs root or a delegated cgroup; if you do not have it,
  the command says so.)

Every action asks for confirmation first. The `--yes` flag skips the question
for scripts, and `--dry-run` works on all of them. A list of protected
processes in the configuration file is never touched by an automatic action.

### Keep an eye on things in the background

`sentinel tui` opens the live dashboard, refreshed once a second (see
[The dashboard](#the-dashboard)). `sentinel daemon` runs headless — no
terminal interface — sampling in the background, raising alerts and applying
policies, for as long as it runs:

```console
$ sentinel daemon --duration 1h --log-file sentinel.log
```

This is the mode used by the systemd service described in
[Running it as a service](#running-it-as-a-service).

### Update it

`sentinel update --check` asks GitHub for the latest release and says whether
it is newer than the binary you are running. `sentinel update` downloads that
release's tarball, checks that the new binary runs, and renames it over the
current one — so it asks first, and needs write access to wherever sentinel is
installed. It uses `curl` and `tar`, and does nothing unless you run it.

### Quick reference

| Command | What it does |
| --- | --- |
| `sentinel health` | One-screen system health summary |
| `sentinel ps [--sort cpu\|mem\|name\|pressure] [--json]` | The process list, sorted by any resource |
| `sentinel top [--json]` | The busiest processes, once, then exit |
| `sentinel tui` | The live dashboard |
| `sentinel watch [--interval 100ms] [--duration 10s]` | High-frequency watch for short-lived spikes |
| `sentinel alerts [--last 1h]` | Recent alerts, read back from the log |
| `sentinel inspect <pid>` | Everything known about one process |
| `sentinel kill <pid> [--signal SIGTERM] [--yes] [--dry-run]` | Safe, confirmed termination |
| `sentinel suspend <pid> [--yes] [--dry-run]` | Stop a process |
| `sentinel resume <pid> [--yes] [--dry-run]` | Continue a process |
| `sentinel renice <pid> <value> [--yes] [--dry-run]` | Change a process priority |
| `sentinel limit <pid> [--cpu 50] [--memory 20] [--remove] [--dry-run]` | Cap what a process may use, with a cgroup |
| `sentinel report [--last 10m] [--json]` | What happened in the recent past |
| `sentinel daemon [--duration] [--background] [--log-file]` | Run in the background |
| `sentinel update [--check] [--yes]` | Install the latest release over this binary |

Every action that touches a process asks first, unless `--yes` is given, and
`--dry-run` reports what would happen without doing it. Every action is
written to the action log.

## The dashboard

`sentinel tui` opens a live dashboard in the spirit of btop: boxes for CPU
(with a meter per core), memory, disk and network, then the recent alerts and
the process table, refreshing once a second. Boxes are drawn with rounded
borders and colour-coded meters (green, yellow, red).

| Key | What it does |
| --- | --- |
| `q` | Quit |
| `t` `m` `i` `N` | Sort by CPU, memory, disk I/O, network |
| `f` | Narrow the table to one resource |
| `/` | Search by process name |
| `↑` `↓` | Move the selection; the table scrolls to follow it |
| `Enter` | Open the detail panel |
| `a` | Open the action menu |
| `k` `s` `r` `n` | Kill, suspend, resume, renice the selected process |

The detail panel shows the command line, the parent, the user, the start
time, the CPU and memory use, the file descriptors, the threads and the
cgroup. The action menu asks before it does anything destructive and offers a
dry-run preview.

## Configuration

The configuration file is looked for in this order:

1. the file named by the `SENTINEL_CONFIG` environment variable
2. `sentinel.toml` in the current directory
3. `$XDG_CONFIG_HOME/sentinel/config.toml`, or `~/.config/sentinel/config.toml`

A `~` in a path is expanded to your home directory. An invalid configuration
is refused at startup, naming the offending key and the line it is on.

```toml
[general]
interval = "1s"                  # how often to sample
history_duration = "10m"         # how much of the recent past to keep
persist_history = true           # keep the history on disk as well
data_dir = "~/.local/share/sentinel"

[ui]
theme = "dark"
show_network = true
show_temperature = true

[sampling]
cpu = true
memory = true
disk = true
network = true
temperature = true
process_io = true
process_network = false           # costs more than the other collectors

[thresholds]
cpu_percent = 90                  # the rules you did not write yourself
memory_percent = 90
temperature_celsius = 85
disk_usage_percent = 90

[critical_processes]
names = ["systemd", "sshd", "init"]   # never touched by an automatic action

[actions]
log_file = "~/.local/state/sentinel/actions.log"
confirm = true

[alerts]
enabled = true
log_file = "~/.local/state/sentinel/alerts.log"

[detection]
oom_evidence = true              # look in the kernel log for the OOM killer
restart_count = 5                 # restarts in the window that count as a loop
restart_window = "1m"

[pressure]                        # the weights of the pressure score
cpu_weight = 1.0
memory_weight = 1.0
io_weight = 1.0
network_weight = 1.0
file_descriptor_weight = 1.0
restart_weight = 1.0

[history]
max_bytes = 10485760             # the history file is trimmed to this size

[daemon]
log_file = "/var/lib/sentinel/sentinel.log"
```

### Weak hardware

```toml
[general]
profile = "low-resource"
```

The low-resource profile forces an interval of at least two seconds, caps the
history at one minute, turns the persistence off and turns the per-process I/O
and network collectors off. Its budget is under 1% of one core and under
20 MiB of memory.

## Alerts

An alert rule watches one metric. It fires only once the condition has held
for the configured duration, so one noisy sample never raises an alert, and
the cooldown keeps it from repeating.

```toml
[[alerts.rules]]
name = "high-cpu"
metric = "cpu_percent"           # or process_cpu_percent, memory_percent,
condition = "above"              # disk_usage_percent, temperature_celsius, ...
threshold = 90
duration = "5s"
severity = "warning"             # info, warning or critical
message = "CPU has been busy for five seconds"
cooldown = "5m"
```

A rule can also be a **spike**: a multiple of a baseline computed over a
recent window.

```toml
[[alerts.rules]]
name = "cpu-spike"
metric = "cpu_percent"
condition = "spike"
severity = "warning"
message = "CPU spiked"

[alerts.rules.spike]
baseline = "moving-average"      # or "ewma"
window = "30s"
multiple = 3.0
minimum = 50.0                   # below this, it is not worth reporting
```

Alerts are printed as they are raised, appended to the alert log as JSON lines
and listed by `sentinel alerts --last 1h`.

## Policies

A policy matches a condition and proposes an action. **Every policy is off
until you turn it on**, and a policy that needs confirmation never acts on its
own.

```toml
[policies]
dry_run = true                   # list the actions, take none of them

[[policies.rules]]
name = "kill-hog"
enabled = false
metric = "process_cpu_percent"
condition = "above"
threshold = 95
duration = "30s"
action = "kill"                  # kill, suspend, resume, renice, throttle
cooldown = "10m"
require_confirmation = false
exclude = ["sshd", "systemd"]    # never these
allow_protected = false          # a protected process is not touched
```

With `dry_run = false` and the policy enabled, the daemon performs the action
and writes it to the action log with the policy that caused it. A protected
process is refused by name unless the policy says it may be touched.

## Running it as a service

The service unit is `packaging/systemd/sentinel.service`. It runs the daemon
as an unprivileged `sentinel` user, restarts it when it fails and writes what
the daemon is doing to `/var/lib/sentinel/sentinel.log`.

```bash
sudo useradd --system --no-create-home sentinel
sudo install -Dm644 packaging/systemd/sentinel.service \
    /etc/systemd/system/sentinel.service
sudo systemctl daemon-reload
sudo systemctl enable --now sentinel
```

The unit points at `/etc/sentinel/config.toml` with `SENTINEL_CONFIG`, and
creates `/etc/sentinel` and `/var/lib/sentinel` itself.

## How it works

```text
  /proc, /sys
       │
       ▼
  ┌─────────────┐   raw counters, one reading per tick
  │  collector  │
  └─────────────┘
       │
       ▼
  ┌─────────────┐   differenced against the previous tick: rates and
  │   sampler   │   percentages, bounded ring buffer of recent samples
  └─────────────┘
       │
   ┌───┴───────────────┬──────────────────┐
   ▼                   ▼                  ▼
┌─────────┐     ┌───────────┐     ┌─────────────┐
│  alerts │     │ policies  │     │    views    │
│  rules  │     │  → actions│     │ CLI · TUI   │
└─────────┘     └───────────┘     │    · daemon │
                                  └─────────────┘
```

Everything reads from the sampler's published state, so the CLI, the dashboard
and the daemon all show the same numbers, and a slow collector can never
freeze the display.

## Development

```bash
cargo build --release     # the release binary
cargo test                # 208 tests
cargo fmt --check         # the formatting check
cargo clippy --all-targets -- -D warnings
make lint                 # the last two together
cargo run --example dashboard   # render the dashboard as text
```

The work is tracked as 32 tracer-bullet tickets in
[`.scratch/system-sentinel/issues/`](.scratch/system-sentinel/issues/), one
commit per ticket.

## Project structure

The source lives in `src/`, organized around the flow shown above — from the
raw system files to what you see on the screen:

```text
src/
├── collect/        # one file per source: /proc, filesystems, network,
│                   # processes, sensors — each knows how to read its files
├── core/           # the metric and formatting types shared by the CLI,
│                   # the dashboard and the daemon
├── tui/            # the live dashboard
├── commands.rs     # the CLI: what each command does, in terms of the below
├── config.rs       # reads and validates sentinel.toml
├── sample.rs       # the sampler: turns raw readings into rates and history
├── alert.rs        # decides when something deserves an alert
├── policy.rs       # decides what to do about it
├── action.rs       # does it: kill, suspend, renice, cgroup limits
├── oom.rs          # reads the kernel log for OOM-killer evidence
├── history.rs      # keeps the recent past, on disk and in memory
└── report.rs       # summarizes that history for `sentinel report`
tests/              # tests that run the real binary end to end
examples/           # dashboard.rs — renders the dashboard without a terminal
```

In short: `collect/` gathers, `sample.rs` measures, `alert.rs` and
`policy.rs` decide, `action.rs` acts, and `commands.rs` plus `tui/` show it.

## Known limits

- **Per-process network traffic** needs something watching the sockets (eBPF).
  Where the platform cannot attribute it, the columns say `n/a` rather than
  showing zero as if it were a measurement.
- **Exit codes and signals** are only visible to a process's own parent, so an
  exit record says *how* the process ended rather than the exact code.
- **Cgroup limits** need root or a delegated cgroup; without them the command
  says so rather than failing quietly.
- **The report** reads the history from disk, so it needs `persist_history` on.
- The packaging is a tarball; there is no `.deb` or `.rpm` yet.

## License

MIT — see [LICENSE.md](LICENSE.md).
