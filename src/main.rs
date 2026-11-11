use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "sentinel",
    version,
    about = "Lightweight Linux system monitor with anomaly detection"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show a one-screen system health summary
    Health,
    /// List processes, sortable by resource usage
    Ps,
    /// Show the busiest processes and exit
    Top,
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

fn main() {
    let command = Cli::parse().command;
    if let Err(error) = sentinel::config::Config::load() {
        eprintln!("sentinel: {error}");
        std::process::exit(1);
    }
    match command {
        Command::Health => not_implemented("health"),
        Command::Ps => not_implemented("ps"),
        Command::Top => not_implemented("top"),
        Command::Tui => not_implemented("tui"),
        Command::Watch => not_implemented("watch"),
        Command::Alerts => not_implemented("alerts"),
        Command::Inspect => not_implemented("inspect"),
        Command::Kill => not_implemented("kill"),
        Command::Suspend => not_implemented("suspend"),
        Command::Resume => not_implemented("resume"),
        Command::Renice => not_implemented("renice"),
        Command::Report => not_implemented("report"),
        Command::Daemon => not_implemented("daemon"),
    }
}

fn not_implemented(subcommand: &str) -> ! {
    eprintln!("sentinel {subcommand}: not implemented yet");
    std::process::exit(1)
}
