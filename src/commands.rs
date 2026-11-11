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
        Command::Watch { interval, duration } => watch(config, interval, duration),
        Command::Alerts { last } => alerts(config, last),
        Command::Inspect { pid } => inspect(config, pid),
        Command::Kill {
            pid,
            signal,
            yes,
            dry_run,
        } => kill(config, pid, signal, yes, dry_run),
        Command::Suspend { pid, yes, dry_run } => {
            signal_action(config, pid, "suspend", libc::SIGSTOP, yes, dry_run)
        }
        Command::Resume { pid, yes, dry_run } => {
            signal_action(config, pid, "resume", libc::SIGCONT, yes, dry_run)
        }
        Command::Renice {
            pid,
            value,
            yes,
            dry_run,
        } => renice(config, pid, value, yes, dry_run),
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
        .with_history(config.general.history_duration)
        .with_detection(
            config.detection.restart_count,
            config.detection.restart_window,
            config.detection.oom_evidence,
        )
        .with_pressure(config.pressure.weights())
        .with_policies(crate::policy::PolicyEngine::new(
            config.policies.rules.clone(),
        ));
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
/// Terminate a process, carefully.
fn kill(
    config: Config,
    pid: u32,
    signal: Option<String>,
    yes: bool,
    dry_run: bool,
) -> Result<(), String> {
    let signal = match signal {
        Some(text) => {
            crate::action::parse_signal(&text).map_err(|error| format!("--signal: {error}"))?
        }
        None => libc::SIGTERM,
    };
    signal_action(config, pid, "kill", signal, yes, dry_run)
}

/// Send a signal to a process, carefully.
fn signal_action(
    config: Config,
    pid: u32,
    action: &str,
    signal: i32,
    yes: bool,
    dry_run: bool,
) -> Result<(), String> {
    let target = check_target(&config, pid, action)?;
    let signal_name = crate::action::signal_name(signal);

    if dry_run {
        let result = "dry run: nothing was sent".to_string();
        record_action(&config, action, pid, &target.name, "user asked", &result);
        println!("would {action} {target} ({pid}) with {signal_name}");
        return Ok(());
    }

    if config.actions.confirm && !yes && !confirm(&format!("{action} {target} ({pid})? [y/N] ")) {
        let result = "refused: the user said no".to_string();
        record_action(&config, action, pid, &target.name, "user asked", &result);
        return Err("cancelled".into());
    }

    // The PID may have been given to another process while we were asking.
    if crate::action::start_time(pid) != Some(target.start_time) {
        let result = "refused: the process changed".to_string();
        record_action(&config, action, pid, &target.name, "user asked", &result);
        return Err(format!(
            "process {pid} changed while confirming, refusing to act on it"
        ));
    }

    match crate::action::send_signal(pid, signal) {
        Ok(()) => {
            let result = format!("sent {signal_name}");
            record_action(&config, action, pid, &target.name, "user asked", &result);
            println!("sent {signal_name} to {target} ({pid})");
            Ok(())
        }
        Err(error) => {
            let result = format!("failed: {error}");
            record_action(&config, action, pid, &target.name, "user asked", &result);
            Err(error)
        }
    }
}

/// Change the priority of a process, carefully.
fn renice(config: Config, pid: u32, value: i32, yes: bool, dry_run: bool) -> Result<(), String> {
    let target = check_target(&config, pid, "renice")?;

    if dry_run {
        let result = "dry run: nothing was changed".to_string();
        record_action(&config, "renice", pid, &target.name, "user asked", &result);
        println!("would renice {target} ({pid}) to {value}");
        return Ok(());
    }

    if config.actions.confirm
        && !yes
        && !confirm(&format!("renice {target} ({pid}) to {value}? [y/N] "))
    {
        let result = "refused: the user said no".to_string();
        record_action(&config, "renice", pid, &target.name, "user asked", &result);
        return Err("cancelled".into());
    }

    if crate::action::start_time(pid) != Some(target.start_time) {
        let result = "refused: the process changed".to_string();
        record_action(&config, "renice", pid, &target.name, "user asked", &result);
        return Err(format!(
            "process {pid} changed while confirming, refusing to act on it"
        ));
    }

    match crate::action::renice(pid, value) {
        Ok(()) => {
            let result = format!("priority set to {value}");
            record_action(&config, "renice", pid, &target.name, "user asked", &result);
            println!("reniced {target} ({pid}) to {value}");
            Ok(())
        }
        Err(error) => {
            let result = format!("failed: {error}");
            record_action(&config, "renice", pid, &target.name, "user asked", &result);
            Err(error)
        }
    }
}

/// The process an action is aimed at.
struct Target {
    name: String,
    /// The start time, so the PID can be checked again before acting.
    start_time: u64,
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

/// Check that the process is there and that it may be touched.
fn check_target(config: &Config, pid: u32, action: &str) -> Result<Target, String> {
    let Some(name) = crate::action::process_name(pid) else {
        return Err(format!("no process {pid}"));
    };
    let Some(start_time) = crate::action::start_time(pid) else {
        return Err(format!("no process {pid}"));
    };
    if let Some(rule) = crate::action::protection(&name, &config.critical_processes.names) {
        let result = format!("refused: protected by the {rule} rule");
        record_action(config, action, pid, &name, "user asked", &result);
        return Err(format!(
            "{name} is protected by the critical_processes rule ({rule})"
        ));
    }
    Ok(Target { name, start_time })
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

/// Watch the metrics as they change.
fn watch(config: Config, interval: Option<String>, duration: Option<String>) -> Result<(), String> {
    let interval = resolve_interval(interval, config.general.interval)?;
    let limit = match duration {
        Some(text) => Some(parse_duration(&text).map_err(|error| format!("--duration: {error}"))?),
        None => None,
    };
    crate::interrupt::install();

    println!(
        "sampling every {} (press Ctrl-C to stop)",
        describe_interval(interval)
    );

    let mut sampler = sampler(&config, interval);
    let deadline = limit.map(|limit| std::time::Instant::now() + limit);
    let stop = || {
        crate::interrupt::requested()
            || deadline.is_some_and(|deadline| std::time::Instant::now() >= deadline)
    };
    let seen_exits = std::cell::Cell::new(0);
    sampler.run_with(&stop, &|state| {
        println!("{}", view::watch_line(&state.system));
        for exit in state.exits.iter().skip(seen_exits.get()) {
            println!("[exit] {}", view::exit_line(exit));
        }
        seen_exits.set(state.exits.len());
        for proposal in &state.proposals {
            println!("[policy] {}", view::proposal_line(proposal));
        }
    });
    Ok(())
}

/// How an interval is shown to the user.
fn describe_interval(interval: Duration) -> String {
    let millis = interval.as_millis();
    if millis < 1_000 {
        format!("{millis}ms")
    } else {
        format!("{:.1}s", interval.as_secs_f64())
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
