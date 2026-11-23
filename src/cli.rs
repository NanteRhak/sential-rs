//! The command-line surface.

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "sentinel",
    version,
    about = "Lightweight Linux system monitor with anomaly detection"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

/// The column the process list is ordered by.
///
/// CPU, memory and pressure sort highest first; name is alphabetical.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum SortKey {
    /// Highest CPU usage first
    Cpu,
    /// Highest resident memory first
    Mem,
    /// Alphabetical by process name
    Name,
    /// Highest pressure first
    Pressure,
}

#[derive(Subcommand)]
pub enum Command {
    /// Show a one-screen system health summary
    ///
    /// Prints a single snapshot and exits: CPU total, per-core and load average
    /// with context-switch and interrupt counts, memory and swap use, disk
    /// throughput, network rates and hardware sensor readings.
    ///
    /// Rates are differences between two readings taken one `--interval` apart,
    /// so the command waits that long before printing. A shorter interval reacts
    /// faster; a longer one smooths spikes out.
    ///
    /// The network row includes loopback traffic only with `--loopback`, since
    /// local traffic would otherwise look like real use. The `ui.show_network`
    /// and `ui.show_temperature` configuration keys can hide those rows
    /// entirely.
    #[command(after_long_help = "Examples:
  sentinel health
  sentinel health --interval 500ms
  sentinel health --loopback")]
    Health {
        /// Sampling interval for this run, for example 500ms or 2s
        ///
        /// Overrides `general.interval` from the configuration file for this
        /// run only. A shorter interval makes the rates more responsive; a
        /// longer one smooths them out.
        #[arg(long, value_name = "DURATION")]
        interval: Option<String>,
        /// Count loopback traffic in the network totals
        ///
        /// Normally loopback interfaces are ignored so local traffic does not
        /// look like real network use.
        #[arg(long)]
        loopback: bool,
    },
    /// List processes, sortable by resource usage
    ///
    /// Prints every process the kernel exposes with PID, user, CPU, memory,
    /// pressure and status columns.
    ///
    /// `--sort` chooses the ordering column. CPU, memory and pressure sort
    /// highest first; name is alphabetical. Ties keep the order the kernel
    /// returned.
    ///
    /// `--json` prints the rows as an array of objects with stable field names,
    /// which is easier to consume from a script than the column layout.
    #[command(after_long_help = "Examples:
  sentinel ps
  sentinel ps --sort mem
  sentinel ps --sort pressure --json")]
    Ps {
        /// Which column to order by
        ///
        /// One of `cpu`, `mem`, `name` or `pressure`. Defaults to `cpu`.
        #[arg(long, value_enum, default_value_t = SortKey::Cpu)]
        sort: SortKey,
        /// Print the rows as JSON
        ///
        /// Useful for scripts that need stable field names rather than the
        /// human-readable table.
        #[arg(long)]
        json: bool,
    },
    /// Show the busiest processes and exit
    ///
    /// A one-shot snapshot of the process table sorted by CPU usage, for quick
    /// checks and scripts. It shows the same rows as `ps --sort cpu` and does
    /// not keep sampling.
    ///
    /// `--json` prints the rows as JSON, just like `ps --json`.
    #[command(after_long_help = "Examples:
  sentinel top
  sentinel top --json")]
    Top {
        /// Print the rows as JSON
        #[arg(long)]
        json: bool,
    },
    /// Open the live terminal dashboard
    ///
    /// Starts the full-screen dashboard: live CPU, memory, network and disk
    /// panels, recent alerts and a process list.
    ///
    /// Keys: `q` quits, `t`/`m`/`i`/`N` change the process sort, `f` filters,
    /// `/` searches, the arrow keys move the selection, `enter` opens process
    /// detail and `a` opens the action menu. The footer always shows the same
    /// hints.
    ///
    /// The dashboard needs a real terminal; it exits when you quit.
    #[command(after_long_help = "Example:
  sentinel tui")]
    Tui,
    /// Sample at a high rate to catch short-lived spikes
    ///
    /// Prints one line per sample and keeps running until Ctrl-C or, when
    /// given, `--duration`. Besides the health line it also reports process
    /// exits, policy proposals and restart loops as they are detected.
    ///
    /// Use a small `--interval` (for example `100ms`) to catch spikes that a
    /// normal `health` run would miss. High rates cost CPU themselves, so
    /// prefer the widest interval that still catches what you are looking for.
    #[command(after_long_help = "Examples:
  sentinel watch
  sentinel watch --interval 100ms
  sentinel watch --interval 250ms --duration 30s")]
    Watch {
        /// How often to sample, for example 100ms
        ///
        /// Overrides `general.interval` for this run. High rates cost more CPU.
        #[arg(long, value_name = "DURATION")]
        interval: Option<String>,
        /// How long to watch for before stopping
        ///
        /// Accepts the same duration syntax as elsewhere, for example `30s` or
        /// `5m`. Without it the command runs until interrupted.
        #[arg(long, value_name = "DURATION")]
        duration: Option<String>,
    },
    /// List recent alerts
    ///
    /// Reads the configured alert log and prints the alerts raised within the
    /// last window, oldest first. Each line includes the severity, time and the
    /// rule that fired.
    ///
    /// If no alert log is configured, the command fails rather than guessing a
    /// location. Configure `alerts.log_file` or run the daemon first.
    #[command(after_long_help = "Examples:
  sentinel alerts
  sentinel alerts --last 30m
  sentinel alerts --last 2h")]
    Alerts {
        /// Only alerts from the last window, for example 1h or 30m
        ///
        /// Defaults to one hour.
        #[arg(long, value_name = "DURATION")]
        last: Option<String>,
    },
    /// Show everything known about one process
    ///
    /// Looks up a single PID and prints a detail view: name, command line,
    /// parent PID, owner, start time and uptime, CPU and memory use, thread and
    /// file-descriptor counts, nice value and cgroup.
    ///
    /// The process must still exist when the sample is taken; a recycled PID is
    /// not detected here, so check the start time before acting on the result.
    #[command(after_long_help = "Examples:
  sentinel inspect 1
  sentinel inspect 4242")]
    Inspect {
        /// The process to look at
        pid: u32,
    },
    /// Terminate a process safely
    ///
    /// Sends a signal to a process after checking that it exists and that its
    /// name is not protected by the `critical_processes` configuration rule.
    /// Before acting, the process start time is read again so a recycled PID
    /// cannot be signalled by mistake. Every attempt, including refusals and
    /// dry runs, is written to the action log.
    ///
    /// `SIGTERM` is the default because it lets the process shut down cleanly.
    /// Use `SIGKILL` only when the process ignores a graceful termination.
    ///
    /// A confirmation is asked unless `actions.confirm` is false or `--yes` is
    /// given. `--dry-run` reports what would happen without sending anything.
    #[command(after_long_help = "Examples:
  sentinel kill 4242
  sentinel kill 4242 --signal SIGKILL --yes
  sentinel kill 4242 --dry-run")]
    Kill {
        /// The process to terminate
        pid: u32,
        /// The signal to send, by name or number
        ///
        /// Accepts names such as `SIGKILL` or numbers such as `9`. Defaults to
        /// `SIGTERM`, which lets the process shut down cleanly.
        #[arg(long, value_name = "SIGNAL")]
        signal: Option<String>,
        /// Do not ask for confirmation
        #[arg(long)]
        yes: bool,
        /// Say what would happen without doing it
        ///
        /// The action is still recorded in the action log.
        #[arg(long)]
        dry_run: bool,
    },
    /// Suspend a process
    ///
    /// Stops a process with `SIGSTOP` after the same existence, protection and
    /// start-time checks as `kill`. The process stays stopped until it is
    /// resumed with `sentinel resume` or another signal continues it.
    ///
    /// This is useful for pausing a noisy process without losing its state.
    #[command(after_long_help = "Examples:
  sentinel suspend 4242
  sentinel suspend 4242 --dry-run")]
    Suspend {
        /// The process to stop
        pid: u32,
        /// Do not ask for confirmation
        #[arg(long)]
        yes: bool,
        /// Say what would happen without doing it
        #[arg(long)]
        dry_run: bool,
    },
    /// Resume a process
    ///
    /// Continues a stopped process with `SIGCONT`, after the same existence,
    /// protection and start-time checks as `kill`. Resuming a process that is
    /// already running does no harm.
    #[command(after_long_help = "Examples:
  sentinel resume 4242
  sentinel resume 4242 --yes")]
    Resume {
        /// The process to continue
        pid: u32,
        /// Do not ask for confirmation
        #[arg(long)]
        yes: bool,
        /// Say what would happen without doing it
        #[arg(long)]
        dry_run: bool,
    },
    /// Cap what a process may use
    ///
    /// Creates a cgroup v2 under `/sys/fs/cgroup/sentinel-<PID>` and moves the
    /// process into it. `--cpu` writes `cpu.max` as a share of one processor;
    /// `--memory` writes `memory.max` as a share of total system RAM.
    ///
    /// At least one of `--cpu`, `--memory` or `--remove` is required. Applying
    /// limits usually needs root or a delegated cgroup. `--remove` moves the
    /// process back to the root cgroup and deletes the created directory.
    ///
    /// Confirmation and `--dry-run` behave as for `kill`, and the outcome is
    /// recorded in the action log.
    #[command(after_long_help = "Examples:
  sentinel limit 4242 --cpu 50
  sentinel limit 4242 --memory 25 --yes
  sentinel limit 4242 --remove")]
    Limit {
        /// The process to cap
        pid: u32,
        /// The share of one processor, as a percentage
        ///
        /// For example `50` allows half of one core, and `200` allows two cores.
        #[arg(long, value_name = "PERCENT")]
        cpu: Option<f64>,
        /// The share of the machine's memory, as a percentage
        ///
        /// For example `25` caps the process at a quarter of total RAM.
        #[arg(long, value_name = "PERCENT")]
        memory: Option<f64>,
        /// Take the limits off again
        #[arg(long)]
        remove: bool,
        /// Do not ask for confirmation
        #[arg(long)]
        yes: bool,
        /// Say what would happen without doing it
        #[arg(long)]
        dry_run: bool,
    },
    /// Change a process priority
    ///
    /// Sets the nice value of a process with the same existence, protection and
    /// start-time checks as `kill`. Values range from `-20` (most important) to
    /// `19` (least important); lower values are more important.
    ///
    /// Lowering the nice value (raising priority) usually needs root. Raising
    /// the nice value is always allowed for your own processes. Quote negative
    /// values so the shell does not read them as options.
    #[command(after_long_help = "Examples:
  sentinel renice 4242 10
  sentinel renice 4242 -5
  sentinel renice 4242 10 --dry-run")]
    Renice {
        /// The process to change
        pid: u32,
        /// The new priority, lower is more important
        value: i32,
        /// Do not ask for confirmation
        #[arg(long)]
        yes: bool,
        /// Say what would happen without doing it
        #[arg(long)]
        dry_run: bool,
    },
    /// Summarise what happened over a time window
    ///
    /// Reads the persisted history and alert log and reports peak and average
    /// CPU and memory, peak load, disk read/write peaks, network peaks and the
    /// alerts raised in the window.
    ///
    /// Persisted history is only available when `general.persist_history` is
    /// enabled and the daemon or another run has written samples. Without
    /// history the report is based on the alerts that remain in the log.
    #[command(after_long_help = "Examples:
  sentinel report
  sentinel report --last 1h
  sentinel report --last 10m --json")]
    Report {
        /// The window to cover, for example 10m
        ///
        /// Defaults to ten minutes.
        #[arg(long, value_name = "DURATION")]
        last: Option<String>,
        /// Print the report as JSON
        #[arg(long)]
        json: bool,
    },
    /// Run in the background
    ///
    /// Starts the sampler as a long-running service, applying the configured
    /// policies and writing alerts and actions where the configuration points.
    /// It prints one line per tick and exits on Ctrl-C or `--duration`.
    ///
    /// `--background` detaches from the terminal with `fork` and `setsid`, so
    /// the process survives its parent shell. `--log-file` also appends the
    /// tick lines to a file; the `daemon.log_file` configuration key sets a
    /// default.
    ///
    /// Run it under systemd for a service that survives reboots. The packaged
    /// unit does this for you.
    #[command(after_long_help = "Examples:
  sentinel daemon
  sentinel daemon --duration 10m
  sentinel daemon --background --log-file /var/log/sentinel.log")]
    Daemon {
        /// How long to run for before stopping
        #[arg(long, value_name = "DURATION")]
        duration: Option<String>,
        /// Detach from the terminal and run on its own
        #[arg(long)]
        background: bool,
        /// Where to write what the daemon is doing
        #[arg(long, value_name = "PATH")]
        log_file: Option<String>,
    },
    /// Update sentinel from its GitHub releases
    ///
    /// Asks the GitHub API for the latest release and, unless `--check` is
    /// given, downloads its tarball with `curl`. The new binary is run with
    /// `--version` before it replaces the current one, and the swap is a
    /// rename on the same filesystem so a half-written binary is never left in
    /// place.
    ///
    /// Replacing a system-wide install usually needs root. `--check` only
    /// reports whether a newer release exists and changes nothing.
    #[command(after_long_help = "Examples:
  sentinel update --check
  sentinel update
  sentinel update --yes")]
    Update {
        /// Only report whether a newer release exists
        #[arg(long)]
        check: bool,
        /// Install without asking for confirmation
        #[arg(long)]
        yes: bool,
    },
    /// Show the manual page
    ///
    /// Prints the bundled manual page, piping it through `man` or `groff` and
    /// a pager when available. `--raw` prints the roff source instead, which is
    /// useful when no formatter is installed or when the page is being
    /// installed as a file.
    #[command(after_long_help = "Examples:
  sentinel man
  sentinel man --raw")]
    Man {
        /// Print raw roff man page source without paging or formatting
        #[arg(long)]
        raw: bool,
    },
}
