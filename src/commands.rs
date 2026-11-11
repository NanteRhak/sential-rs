//! What each subcommand does.

use std::time::Duration;

use crate::alert::{AlertEngine, append_alert, read_alerts};
use crate::cli::Command;
use crate::collect::proc::ProcCollector;
use crate::config::{Config, parse_duration};
use crate::sample::Sampler;
use crate::view;

/// Run one command. `Err` is a message for the user and a non-zero exit.
pub fn run(command: Command, config: Config) -> Result<(), String> {
    match command {
        Command::Health { interval, loopback } => health(config, interval, loopback),
        Command::Ps { sort, json } => ps(config, sort, json),
        Command::Top { json } => top(config, json),
        Command::Tui => crate::tui::run(config),
        Command::Watch => Err("watch is not implemented yet".into()),
        Command::Alerts { last } => alerts(config, last),
        Command::Inspect { pid } => inspect(config, pid),
        Command::Kill { pid, yes, dry_run } => kill(config, pid, yes, dry_run),
        Command::Suspend => Err("suspend is not implemented yet".into()),
        Command::Resume => Err("resume is not implemented yet".into()),
        Command::Renice => Err("renice is not implemented yet".into()),
        Command::Report => Err("report is not implemented yet".into()),
        Command::Daemon => Err("daemon is not implemented yet".into()),
    }
}

fn health(config: Config, interval: Option<String>, loopback: bool) -> Result<(), String> {
    let interval = resolve_interval(interval, config.general.interval)?;
    let mut sampler = sampler(&config, interval);
    if loopback {
        sampler = sampler.with_collector(Box::new(ProcCollector::new(
            crate::collect::proc::ProcFs::system(),
            true,
        )));
    }
    let state = sampler.warm_sample();
    if let Some(error) = sampler.last_error() {
        return Err(format!("cannot read system metrics: {error}"));
    }
    print!(
        "{}",
        view::health(
            &state.system,
            config.ui.show_network,
            config.ui.show_temperature,
        )
    );
    Ok(())
}

fn ps(config: Config, sort: crate::cli::SortKey, json: bool) -> Result<(), String> {
    let mut processes = sample_processes(&config)?;
    view::sort_processes(&mut processes, view::Sort::of(sort));
    if json {
        println!("{}", view::processes_json(&processes));
    } else {
        print!("{}", view::processes(&processes));
    }
    Ok(())
}

fn inspect(config: Config, pid: u32) -> Result<(), String> {
    let processes = sample_processes(&config)?;
    let Some(process) = processes.iter().find(|process| process.pid == pid) else {
        return Err(format!("no process {pid}"));
    };
    print!("{}", view::process_detail(process));
    Ok(())
}

fn top(config: Config, json: bool) -> Result<(), String> {
    let mut processes = sample_processes(&config)?;
    view::sort_processes(&mut processes, view::Sort::Cpu);
    if json {
        println!("{}", view::processes_json(&processes));
    } else {
        print!("{}", view::processes(&processes));
    }
    Ok(())
}

/// A sampler that watches the configured rules and prints the alerts it
/// raises as they happen.
pub fn sampler(config: &Config, interval: std::time::Duration) -> Sampler {
    let sampler = Sampler::new(interval, Box::new(ProcCollector::default()))
        .with_history(config.general.history_duration);
    if !config.alerts.enabled || config.alerts.rules.is_empty() {
        return sampler;
    }
    let log_file = config.alerts.log_file.clone();
    sampler.with_alerts(
        AlertEngine::new(config.alerts.rules.clone()),
        Box::new(move |alert| {
            println!("{}", view::alert_line(alert));
            if let Some(path) = &log_file
                && let Err(error) = append_alert(path, alert)
            {
                eprintln!("sentinel: {error}");
            }
        }),
    )
}

/// Terminate a process, carefully.
fn kill(config: Config, pid: u32, yes: bool, dry_run: bool) -> Result<(), String> {
    let Some(name) = crate::action::process_name(pid) else {
        return Err(format!("no process {pid}"));
    };
    let Some(start_time) = crate::action::start_time(pid) else {
        return Err(format!("no process {pid}"));
    };

    if let Some(rule) = crate::action::protection(&name, &config.critical_processes.names) {
        let result = format!("refused: protected by the {rule} rule");
        record_action(&config, "kill", pid, &name, "user asked", &result);
        return Err(format!(
            "{name} is protected by the critical_processes rule ({rule})"
        ));
    }

    if dry_run {
        let result = "dry run: nothing was sent".to_string();
        record_action(&config, "kill", pid, &name, "user asked", &result);
        println!("would kill {name} ({pid}) with SIGTERM");
        return Ok(());
    }

    if config.actions.confirm && !yes && !confirm(&format!("Kill {name} ({pid})? [y/N] ")) {
        let result = "refused: the user said no".to_string();
        record_action(&config, "kill", pid, &name, "user asked", &result);
        return Err("cancelled".into());
    }

    // The PID may have been given to another process while we were asking.
    if crate::action::start_time(pid) != Some(start_time) {
        let result = "refused: the process changed".to_string();
        record_action(&config, "kill", pid, &name, "user asked", &result);
        return Err(format!(
            "process {pid} changed while confirming, refusing to kill it"
        ));
    }

    match crate::action::send_signal(pid, libc::SIGTERM) {
        Ok(()) => {
            let result = "sent SIGTERM".to_string();
            record_action(&config, "kill", pid, &name, "user asked", &result);
            println!("sent SIGTERM to {name} ({pid})");
            Ok(())
        }
        Err(error) => {
            let result = format!("failed: {error}");
            record_action(&config, "kill", pid, &name, "user asked", &result);
            Err(error)
        }
    }
}

/// Ask the user a yes or no question.
fn confirm(question: &str) -> bool {
    use std::io::Write;
    print!("{question}");
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    if std::io::stdin().read_line(&mut answer).is_err() {
        return false;
    }
    matches!(answer.trim(), "y" | "Y" | "yes" | "Yes")
}

/// Write an action to the action log.
fn record_action(
    config: &Config,
    action: &str,
    pid: u32,
    process_name: &str,
    reason: &str,
    result: &str,
) {
    let Some(path) = &config.actions.log_file else {
        return;
    };
    let record = crate::action::ActionRecord {
        timestamp: crate::collect::now_millis(),
        action: action.to_string(),
        pid,
        process_name: process_name.to_string(),
        reason: reason.to_string(),
        user_requested: true,
        result: result.to_string(),
    };
    if let Err(error) = crate::action::write_record(path, &record) {
        eprintln!("sentinel: {error}");
    }
}

/// The alerts of the last window, read back from the log file.
fn alerts(config: Config, last: Option<String>) -> Result<(), String> {
    let window = match last {
        Some(text) => parse_duration(&text).map_err(|error| format!("--last: {error}"))?,
        None => Duration::from_secs(3_600),
    };
    let Some(path) = config.alerts.log_file else {
        return Err("no alert log is configured".into());
    };
    let cutoff = crate::collect::now_millis().saturating_sub(window.as_millis() as u64);
    let recent: Vec<_> = read_alerts(&path)
        .into_iter()
        .filter(|alert| alert.timestamp >= cutoff)
        .collect();
    if recent.is_empty() {
        return Ok(());
    }
    for alert in recent {
        println!("{}", view::alert_line(&alert));
    }
    Ok(())
}

/// Two readings one interval apart, so the per-process rates are meaningful.
fn sample_processes(config: &Config) -> Result<Vec<crate::core::process::ProcessSample>, String> {
    let mut sampler = sampler(config, config.general.interval);
    let state = sampler.warm_sample();
    if let Some(error) = sampler.last_error() {
        return Err(format!("cannot read system metrics: {error}"));
    }
    Ok(state.processes.clone())
}

/// The interval for this run: the command line wins over the configuration.
fn resolve_interval(text: Option<String>, configured: Duration) -> Result<Duration, String> {
    match text {
        None => Ok(configured),
        Some(text) => parse_duration(&text).map_err(|error| format!("--interval: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_line_interval_wins_over_the_configuration() {
        let configured = Duration::from_secs(5);

        let interval = resolve_interval(Some("250ms".into()), configured).unwrap();

        assert_eq!(interval, Duration::from_millis(250));
    }

    #[test]
    fn without_an_override_the_configured_interval_is_used() {
        let configured = Duration::from_secs(5);

        let interval = resolve_interval(None, configured).unwrap();

        assert_eq!(interval, configured);
    }

    #[test]
    fn a_bad_interval_is_a_clear_error() {
        let error = resolve_interval(Some("fast".into()), Duration::from_secs(1)).unwrap_err();

        assert!(error.contains("--interval"), "{error}");
    }
}
