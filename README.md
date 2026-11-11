1| # Rust System Sentinel
2| 
3| **A lightweight Linux system monitor with anomaly detection and safe resource control.**
4| htop's view of the machine, plus the things htop cannot tell you: what happened
5| ten seconds ago, which process spiked for 200 ms, and what to do about it.
6| 
7| [![Rust](https://img.shields.io/badge/rust-2024--orange?style=flat-square)](https://www.rust-lang.org/)
8| [![Linux](https://img.shields.io/badge/platform-linux-lightgrey?style=flat-square)](https://www.kernel.org/)
9| [![Dependencies](https://img.shields.io/badge/dependencies-7-informational?style=flat-square)](Cargo.toml)
10| 
11| ```text
12| ┌ System ─────────────────────────────────────────────────────────────��[...]
13| │CPU           13.1%  user 9.1%  system 6.2%  iowait 0.4%  idle 81.6%                                  │
14| │Memory        7.7 GiB used of 14.8 GiB, 6.8 GiB available, 1.0 GiB swap                               │
15| │Disk          read 1.0 MiB  write 12.0 MiB                                                            │
16| │Network       received 512.0 KiB  sent 128.0 KiB                                                      │
17| │Temperature   x86_pkg_temp 57.0°C                                                                     │
18| │Load          1.42 1.10 0.87                                                                          │
19| └────────────────────────────────────────────────────────────────�[...]
20| ┌ Alerts ─────────────────────────────────────────────────────────────��[...]
21| │no alerts                                                                                             │
22| │                                                                                                      │
23| └────────────────────────────────────────────────────────────────�[...]
24| ┌ CPU % ─────────────────┐┌ Memory % ──────────────┐┌ Disk bytes/s ──────────┐┌ Network bytes/s ─�[...]
25| │                        ││                        ││████████████████████████││██████████████████�[...]
26| │                        ││                        ││████████████████████████││██████████████████�[...]
27| │                        ││      ▁      ▁      ▁   ││████████████████████████││████████████████�[...]
28| │                        ││████████████████████████││████████████████████████││██�[...]
29| │       ▁▂▃▄▄       ▁▂▃▄▄││████████████████████████││█████████████████████��[...]
30| │▃▄▅▆▇███████▃▄▅▆▇███████││████████████████████████││████████████�[...]
31| └────────────────────────┘└────────────────────────┘└────────────�[...]
32| ┌ Processes (5) | sort: cpu ───────────────────────────────────────────────────────�[...]
33| │PID      USER         CPU%    MEM%    NAME                                                            │
34| │5        you          44.7    32.0    rustc                                                           │
35| │2        you          18.4    38.2    firefox                                                         │
36| │3        you          9.1     64.0    backup-agent                                                    │
37| │4        postgres     2.2     16.0    postgres                                                        │
38| │1        root         0.2     0.8     systemd                                                         │
39| │                                                                                                      │
40| │                                                                                                      │
41| │                                                                                                      │
42| │                                                                                                      │
43| │                                                                                                      │
44| │                                                                                                      │
45| └────────────────────────────────────────────────────────────────�[...]
46| ```
47| 
48| <sub>The picture above is rendered by `cargo run --example dashboard`, so it is the
49| real dashboard and not a drawing of one.</sub>
50| 
51| - [Why](#why)
52| - [What it does](#what-it-does)
53| - [Install](#install)
54| - [Use it](#use-it)
55| - [Commands](#commands)
56| - [The dashboard](#the-dashboard)
57| - [Configuration](#configuration)
58| - [Alerts](#alerts)
59| - [Policies](#policies)
60| - [Running it as a service](#running-it-as-a-service)
61| - [How it works](#how-it-works)
62| - [Development](#development)
63| - [Known limits](#known-limits)
64| - [License](#license)
65| 
66| ## Why
67| 
68| - The system slows down and nobody knows why.
69| - An app crashes unexpectedly and the culprit is hard to find.
70| - Some anomalies last milliseconds and slip past a monitor that samples once a
71|   second.
72| - On weak hardware, resources have to be rationed and critical processes kept
73|   alive.
74| 
75| ## What it does
76| 
77| - **System health** — CPU (total, per core, user/system/iowait/idle), load
78|   average, memory, swap, disk throughput and filesystem usage, network
79|   throughput and packet rate, temperatures, fans, throttling, kernel pressure
80|   stalls.
81| - **Process view** — CPU%, RSS/VSZ, per-process disk I/O, thread and
82|   file-descriptor counts, state, user, cgroup, and one **pressure score** that
83|   combines everything into a single number.
84| - **Anomaly detection** — threshold rules, spike detection against moving
85|   baselines, unexpected-exit detection, restart-loop detection and
86|   OOM-killer evidence.
87| - **High-resolution sampling** — down to 100 ms, with a bounded ring buffer so
88|   a 200 ms spike is still there when you go looking for it.
89| - **Alerts** — severity, category, value against threshold, suggested action.
90|   Shown on the terminal, written to a log file, listed in the dashboard.
91| - **Resource control** — kill, signal, suspend, resume, renice and cgroup
92|   limits. Every action asks first, protected processes are never killed
93|   automatically, and every action is written down.
94| - **Three interfaces** — a CLI for scripting, a live dashboard, and a daemon
95|   for running unattended.
96| 
97| Everything is read from `/proc` and `/sys`. Nothing leaves the machine, and
98| nothing is sent anywhere.
99| 
100| ## Install
101| 
102| From source:
103| 
104| ```bash
105| git clone <your fork> rust-system-sentinel
106| cd rust-system-sentinel
107| cargo install --path .
108| ```
109| 
110| From a release tarball:
111| 
112| ```bash
113| make package                       # builds build/sentinel-<version>.tar.gz
114| tar -C build -xzf build/sentinel-*.tar.gz
115| sudo make install                  # into /usr/local
116| ```
117| 
118| `make install` puts the binary in `/usr/local/bin` and the service unit in
119| `/usr/local/lib/systemd/system`. There is no runtime dependency beyond libc:
120| `clap`, `serde`, `toml`, `ratatui`, `crossterm` and `serde_json` are compiled in.
121| 
122| ## Use it
123| 
124| A one-screen summary of the machine:
125| 
126| ```console
127| $ sentinel health
128| CPU
129|   Total             32.1%  user 28.3%  system 3.8%  iowait 0.0%  idle 67.9%
130|   Per-core          7.7% 7.1% 88.5% 11.5%
131|   Load average      0.70 0.61 0.65
132|   Context switches  152307281  interrupts 92848853
133| 
134| Memory
135|   Total             7.6 GiB
136|   Used              3.6 GiB (47.5%)
137|   Available         4.0 GiB
138|   Cached            3.6 GiB
139|   Buffers           88.9 MiB
140|   Swap              7.6 GiB total, 1.0 GiB used
141| 
142| Disk
143|   Throughput        read 0 B/s  write 0 B/s
144|   Operations        read 0/s  write 0/s
145|   Device sda        read 0 B/s  write 0 B/s  0 ops read/s  0 ops write/s
146|   Filesystem /      136.5 GiB used of 168.6 GiB (81.0%)
147|   Filesystem /boot/efi24.6 MiB used of 511.0 MiB (4.8%)
148|   Filesystem /run/media/nante/Stockage1567.6 GiB used of 710.8 GiB (79.8%)
149| 
150| Network
151|   Throughput        received 0 B/s  sent 431 B/s
152|   Packets           0/s received  3/s sent
153|   Errors            0  drops 0
154|   Interface enp3s0  received 0 B/s  sent 0 B/s  0/s in  0/s out  errors 0  drops 0
155|   Interface wlp2s0  received 0 B/s  sent 431 B/s  0/s in  3/s out  errors 0  drops 0
156| 
157| Sensors
158|   Temperature       acpitz 52.0°C  x86_pkg_temp 60.0°C  hwmon1 temp1 52.0°C  hwmon1 temp2 52.0°C  Package id 0 64.0°C  Core 0 63.0°C  Core 1 63.0°C
159|   Fans              no readable sensors
160|   Throttling        no
161|   Pressure stall    cpu 0.4%  memory 0.0%  io 1.0%
162| ```
163| 
164| The processes, ordered by the resource you care about:
165| 
166| ```console
167| $ sentinel ps --sort pressure
168|     PID     PPID  USER        STATE    CPU%    ELAPSED         RSS         VSZ    MEM%     PRS  NAME
169|   92063        1  nante       S        0.9    4:49:27   599.8 MiB    20.4 GiB    7.7%    3.1  firefox
170|   98328    97033  nante       S        4.7    2:53:55   653.7 MiB    13.3 GiB    8.4%    2.1  opencode
171|  243358   243347  nante       R        4.7      00:01     4.6 MiB     5.8 MiB    0.1%    1.0  sentinel
172| ```
173| 
174| The same list, for a script:
175| 
176| ```console
177| $ sentinel ps --json | jq -r '.processes[] | select(.cpu_percent > 0) | .name' | head -3
178| firefox
179| Isolated Web Co
180| sentinel
181| ```
182| 
183| What happened in the last ten minutes:
184| 
185| ```console
186| $ sentinel report --last 10m
187| Report for the last 10:00 (11 samples)
188|   CPU               peak 35.0%  average 25.3%
189|   Memory            peak 47.6%  average 47.5%
190|   Load              peak 0.54
191|   Disk              read peak 0 B  write peak 0 B
192|   Network           received peak 2.9 KiB  sent peak 1.0 KiB
193|   Process           Isolated Web Co (96445) cpu 63.2%  memory 9.1%
194|   Process           firefox (92063) cpu 50.8%  memory 7.7%
194|   Process           sentinel (245393) cpu 26.0%  memory 0.1%
195| ```
196| 
197| And a runaway process, stopped safely:
198| 
199| ```console
200| $ sentinel kill 12345
201| Kill backup-agent (12345)? [y/N] y
202| sent SIGTERM to backup-agent (12345)
203| ```
204| 
205| ## Commands
206| 
207| | Command | What it does |
208| | --- | --- |
209| | `sentinel health` | One-screen system health summary |
210| | `sentinel ps [--sort cpu\|mem\|name\|pressure] [--json]` | The process list, sorted by any resource |
211| | `sentinel top [--json]` | The busiest processes, once, then exit |
212| | `sentinel tui` | The live dashboard |
213| | `sentinel watch [--interval 100ms] [--duration 10s]` | High-frequency watch for short-lived spikes |
214| | `sentinel alerts [--last 1h]` | Recent alerts, read back from the log |
214| | `sentinel inspect <pid>` | Everything known about one process |
215| | `sentinel kill <pid> [--signal SIGTERM] [--yes] [--dry-run]` | Safe, confirmed termination |
216| | `sentinel suspend <pid> [--yes] [--dry-run]` | Stop a process |
217| | `sentinel resume <pid> [--yes] [--dry-run]` | Continue a process |
218| | `sentinel renice <pid> <value> [--yes] [--dry-run]` | Change a process priority |
219| | `sentinel limit <pid> [--cpu 50] [--memory 20] [--remove] [--dry-run]` | Cap what a process may use, with a cgroup |
220| | `sentinel report [--last 10m] [--json]` | What happened in the recent past |
221| | `sentinel daemon [--duration] [--background] [--log-file]` | Run in the background |
222| 
223| Every action that touches a process asks first, unless `--yes` is given, and
224| `--dry-run` reports what would happen without doing it. Every action is
225| written to the action log.
226| 
227| ## The dashboard
228| 
229| `sentinel tui` opens a live dashboard: the system summary, the recent alerts,
230| four usage graphs and the process table, refreshing once a second.
231| 
232| | Key | What it does |
233| | --- | --- |
234| | `q` | Quit |
235| | `t` `m` `i` `N` | Sort by CPU, memory, disk I/O, network |
236| | `f` | Narrow the table to one resource |
237| | `/` | Search by process name |
238| | `↑` `↓` | Move the selection |
239| | `Enter` | Open the detail panel |
240| | `a` | Open the action menu |
241| | `k` `s` `r` `n` | Kill, suspend, resume, renice the selected process |
242| 
243| The detail panel shows the command line, the parent, the user, the start
244| time, the CPU and memory use, the file descriptors, the threads and the
245| cgroup. The action menu asks before it does anything destructive and offers a
246| dry-run preview.
247| 
248| ## Configuration
249| 
250| The configuration file is looked for in this order:
251| 
252| 1. the file named by the `SENTINEL_CONFIG` environment variable
253| 2. `sentinel.toml` in the current directory
254| 3. `$XDG_CONFIG_HOME/sentinel/config.toml`, or `~/.config/sentinel/config.toml`
255| 
256| A `~` in a path is expanded to your home directory. An invalid configuration
257| is refused at startup, naming the offending key and the line it is on.
258| 
259| ```toml
260| [general]
261| interval = "1s"                  # how often to sample
262| history_duration = "10m"         # how much of the recent past to keep
263| persist_history = true           # keep the history on disk as well
264| data_dir = "~/.local/share/sentinel"
265| 
266| [ui]
267| theme = "dark"
268| show_network = true
269| show_temperature = true
270| 
271| [sampling]
272| cpu = true
273| memory = true
274| disk = true
275| network = true
276| temperature = true
277| process_io = true
278| process_network = false           # costs more than the other collectors
279| 
280| [thresholds]
281| cpu_percent = 90                  # the rules you did not write yourself
282| memory_percent = 90
283| temperature_celsius = 85
284| disk_usage_percent = 90
285| 
286| [critical_processes]
287| names = ["systemd", "sshd", "init"]   # never touched by an automatic action
288| 
289| [actions]
290| log_file = "~/.local/state/sentinel/actions.log"
291| confirm = true
292| 
293| [alerts]
294| enabled = true
295| log_file = "~/.local/state/sentinel/alerts.log"
296| 
297| [detection]
298| oom_evidence = true              # look in the kernel log for the OOM killer
299| restart_count = 5                 # restarts in the window that count as a loop
300| restart_window = "1m"
301| 
302| [pressure]                        # the weights of the pressure score
303| cpu_weight = 1.0
304| memory_weight = 1.0
305| io_weight = 1.0
306| network_weight = 1.0
307| file_descriptor_weight = 1.0
308| restart_weight = 1.0
309| 
310| [history]
311| max_bytes = 10485760             # the history file is trimmed to this size
312| 
313| [daemon]
314| log_file = "/var/lib/sentinel/sentinel.log"
315| ```
316| 
317| ### Weak hardware
318| 
319| ```toml
320| [general]
321| profile = "low-resource"
322| ```
323| 
324| The low-resource profile forces an interval of at least two seconds, caps the
325| history at one minute, turns the persistence off and turns the per-process I/O
326| and network collectors off. Its budget is under 1% of one core and under
327| 20 MiB of memory.
328| 
329| ## Alerts
330| 
331| An alert rule watches one metric. It fires only once the condition has held
332| for the configured duration, so one noisy sample never raises an alert, and
333| the cooldown keeps it from repeating.
334| 
335| ```toml
336| [[alerts.rules]]
337| name = "high-cpu"
338| metric = "cpu_percent"           # or process_cpu_percent, memory_percent,
339| condition = "above"              # disk_usage_percent, temperature_celsius, ...
340| threshold = 90
341| duration = "5s"
342| severity = "warning"             # info, warning or critical
343| message = "CPU has been busy for five seconds"
344| cooldown = "5m"
345| ```
346| 
347| A rule can also be a **spike**: a multiple of a baseline computed over a
348| recent window.
349| 
350| ```toml
351| [[alerts.rules]]
352| name = "cpu-spike"
353| metric = "cpu_percent"
354| condition = "spike"
355| severity = "warning"
356| message = "CPU spiked"
357| 
358| [alerts.rules.spike]
359| baseline = "moving-average"      # or "ewma"
360| window = "30s"
361| multiple = 3.0
362| minimum = 50.0                   # below this, it is not worth reporting
363| ```
364| 
365| Alerts are printed as they are raised, appended to the alert log as JSON lines
366| and listed by `sentinel alerts --last 1h`.
367| 
368| ## Policies
369| 
370| A policy matches a condition and proposes an action. **Every policy is off
371| until you turn it on**, and a policy that needs confirmation never acts on its
372| own.
373| 
374| ```toml
375| [policies]
376| dry_run = true                   # list the actions, take none of them
377| 
378| [[policies.rules]]
379| name = "kill-hog"
380| enabled = false
381| metric = "process_cpu_percent"
382| condition = "above"
383| threshold = 95
384| duration = "30s"
385| action = "kill"                  # kill, suspend, resume, renice, throttle
386| cooldown = "10m"
387| require_confirmation = false
388| exclude = ["sshd", "systemd"]    # never these
389| allow_protected = false          # a protected process is not touched
390| ```
391| 
392| With `dry_run = false` and the policy enabled, the daemon performs the action
393| and writes it to the action log with the policy that caused it. A protected
394| process is refused by name unless the policy says it may be touched.
395| 
396| ## Running it as a service
397| 
398| The service unit is `packaging/systemd/sentinel.service`. It runs the daemon
399| as an unprivileged `sentinel` user, restarts it when it fails and writes what
400| the daemon is doing to `/var/lib/sentinel/sentinel.log`.
401| 
402| ```bash
403| sudo useradd --system --no-create-home sentinel
404| sudo install -Dm644 packaging/systemd/sentinel.service \
405|     /etc/systemd/system/sentinel.service
406| sudo systemctl daemon-reload
407| sudo systemctl enable --now sentinel
408| ```
409| 
410| The unit points at `/etc/sentinel/config.toml` with `SENTINEL_CONFIG`, and
411| creates `/etc/sentinel` and `/var/lib/sentinel` itself.
412| 
413| ## How it works
414| 
415| ```text
416|   /proc, /sys
417|        │
418|        ▼
419|   ┌─────────────┐   raw counters, one reading per tick
420|   │  collector  │
421|   └─────────────┘
422|        │
423|        ▼
424|   ┌─────────────┐   raw counters, one reading per tick
425|   │   sampler   │   percentages, bounded ring buffer of recent samples
426|   └─────────────┘
427|        │
428|    ┌───┴───────────────┬──────────────────┐
429|    ▼                   ▼                  ▼
430| ┌─────────┐     ┌───────────┐     ┌─────────────┐
431| │  alerts │     │ policies  │     │    views    │
432| │  rules  │     │  → actions│     │ CLI · TUI   │
433| └─────────┘     └───────────┘     │    · daemon │
434|                                   └─────────────┘
435| ```
436| 
437| Everything reads from the sampler's published state, so the CLI, the dashboard
438| and the daemon all show the same numbers, and a slow collector can never
439| freeze the display.
440| 
441| ## Development
442| 
443| ```bash
444| cargo build --release     # the release binary
445| cargo test                # 208 tests
446| cargo fmt --check         # the formatting check
447| cargo clippy --all-targets -- -D warnings
448| make lint                 # the last two together
449| cargo run --example dashboard   # render the dashboard as text
450| ```
451| 
452| The work is tracked as 32 tracer-bullet tickets in
453| [`.scratch/system-sentinel/issues/`](.scratch/system-sentinel/issues/), one
454| commit per ticket.
455| 
456| ## Known limits
457| 
458| - **Per-process network traffic** needs something watching the sockets (eBPF).
459|   Where the platform cannot attribute it, the columns say `n/a` rather than
460|   showing zero as if it were a measurement.
461| - **Exit codes and signals** are only visible to a process's own parent, so an
462|   exit record says *how* the process ended rather than the exact code.
463| - **Cgroup limits** need root or a delegated cgroup; without them the command
464|   says so rather than failing quietly.
465| - **The report** reads the history from disk, so it needs `persist_history` on.
466| - The packaging is a tarball; there is no `.deb` or `.rpm` yet.
467| 
468| ## License
469| 
470| This project is licensed under the MIT License. See the [LICENSE](LICENSE)
| file for details.
472| 
