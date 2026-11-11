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
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum SortKey {
    Cpu,
    Mem,
    Name,
}

#[derive(Subcommand)]
pub enum Command {
    /// Show a one-screen system health summary
    Health {
        /// Sampling interval for this run, for example 500ms or 2s
        #[arg(long, value_name = "DURATION")]
        interval: Option<String>,
        /// Count loopback traffic in the network totals
        #[arg(long)]
        loopback: bool,
    },
    /// List processes, sortable by resource usage
    Ps {
        /// Which column to order by
        #[arg(long, value_enum, default_value_t = SortKey::Cpu)]
        sort: SortKey,
        /// Print the rows as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show the busiest processes and exit
    Top {
        /// Print the rows as JSON
        #[arg(long)]
        json: bool,
    },
    /// Open the live terminal dashboard
    Tui,
    /// Sample at a high rate to catch short-lived spikes
    Watch,
    /// List recent alerts
    Alerts,
    /// Show everything known about one process
    Inspect,
    /// Terminate a process safely
    Kill,
    /// Suspend a process
    Suspend,
    /// Resume a process
    Resume,
    /// Change a process priority
    Renice,
    /// Summarise what happened over a time window
    Report,
    /// Run in the background
    Daemon,
}
