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
        Command::Limit {
            pid,
            cpu,
            memory,
            remove,
            yes,
            dry_run,
        } => limit(config, pid, cpu, memory, remove, yes, dry_run),
        Command::Renice {
            pid,
            value,
            yes,
            dry_run,
        } => renice(config, pid, value, yes, dry_run),
        Command::Report { last, json } => report(config, last, json),
        Command::Daemon {
            duration,
            background,
            log_file,
        } => daemon(config, duration, background, log_file),
    }
}

fn health(config: Config, interval: Option<String>, loopback: bool) -> Result<(), String> {
    let interval = resolve_interval(interval, config.general.interval)?;
    let mut sampler = sampler(&config, interval, print_alerts(&config));
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
        print!(
            "{}",
            view::processes(
                &processes,
                &config.critical_processes.names,
                config.sampling.process_network,
            )
        );
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
        print!(
            "{}",
            view::processes(
                &processes,
                &config.critical_processes.names,
                config.sampling.process_network,
            )
        );
    }
    Ok(())
}

/// A sampler that watches the configured rules and prints the alerts it
/// raises as they happen.
pub fn sampler(
    config: &Config,
    interval: std::time::Duration,
    on_alert: crate::sample::AlertSink,
) -> Sampler {
    let mut sampler = Sampler::new(interval, Box::new(ProcCollector::default()))
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

    if config.alerts.enabled {
        // The thresholds are the rules the user did not write themselves.
        let rules = if config.alerts.rules.is_empty() {
            crate::alert::default_rules(&config.thresholds)
        } else {
            config.alerts.rules.clone()
        };
        sampler = sampler.with_alerts(AlertEngine::new(rules), on_alert);
    }

    if config.general.persist_history
        && let Some(store) = history_store(config)
    {
        sampler = sampler.with_history_store(
            store.with_max_bytes(config.history.max_bytes),
            Duration::from_secs(5),
        );
    }

    sampler
}

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

/// Cap what a process may use, with a cgroup.
fn limit(
    config: Config,
    pid: u32,
    cpu: Option<f64>,
    memory: Option<f64>,
    remove: bool,
    yes: bool,
    dry_run: bool,
) -> Result<(), String> {
    let target = check_target(&config, pid, "limit")?;
    let root = std::path::PathBuf::from("/sys/fs/cgroup");
    let limits = crate::cgroup::Limits {
        cpu_percent: cpu,
        memory_percent: memory,
    };

    if remove {
        if dry_run {
            println!("would take the limits off {target} ({pid})");
            return Ok(());
        }
        let outcome = crate::cgroup::release(&root, pid)?;
        record_action(&config, "limit", pid, &target.name, "user asked", &outcome);
        println!("{outcome}");
        return Ok(());
    }

    if limits.is_empty() {
        return Err("no limit was given: use --cpu, --memory or --remove".into());
    }

    if config.actions.confirm && !yes && !confirm(&format!("limit {target} ({pid})? [y/N] ")) {
        let result = "refused: the user said no".to_string();
        record_action(&config, "limit", pid, &target.name, "user asked", &result);
        return Err("cancelled".into());
    }

    if dry_run {
        let result = "dry run: nothing was changed".to_string();
        record_action(&config, "limit", pid, &target.name, "user asked", &result);
        println!("would limit {target} ({pid}) to {limits:?}");
        return Ok(());
    }

    let total_memory = crate::collect::proc::ProcFs::system()
        .memory()
        .map(|memory| memory.total_bytes)
        .unwrap_or(0);
    let outcome = crate::cgroup::apply(&root, pid, &limits, total_memory)?;
    record_action(&config, "limit", pid, &target.name, "user asked", &outcome);
    println!("{outcome}");
    Ok(())
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
    let Some(path) = &config.actions.log_path() else {
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

    let mut sampler = sampler(&config, interval, print_alerts(&config));
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
        for loop_ in &state.restart_loops {
            println!("[restart] {}", view::restart_loop_line(loop_));
        }
    });
    sampler.flush_history();
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

/// Run in the background, collecting and watching.
fn daemon(
    config: Config,
    duration: Option<String>,
    background: bool,
    log_file: Option<String>,
) -> Result<(), String> {
    let limit = match duration {
        Some(text) => Some(parse_duration(&text).map_err(|error| format!("--duration: {error}"))?),
        None => None,
    };
    if background {
        detach()?;
    }
    crate::interrupt::install();
    // The daemon should survive the terminal that started it going away.
    crate::interrupt::ignore_hangup();

    let interval = config.general.interval;
    let mut sampler = sampler(&config, interval, print_alerts(&config)).with_executor(
        crate::action::Executor::new(
            config.critical_processes.names.clone(),
            config.actions.log_path(),
            config.policies.dry_run,
        ),
    );

    let log_file = log_file
        .map(std::path::PathBuf::from)
        .or_else(|| config.daemon.log_file.clone());
    println!(
        "sentinel daemon: sampling every {}",
        describe_interval(interval)
    );

    let deadline = limit.map(|limit| std::time::Instant::now() + limit);
    let stop = || {
        crate::interrupt::requested()
            || deadline.is_some_and(|deadline| std::time::Instant::now() >= deadline)
    };
    sampler.run_with(&stop, &|state| {
        let memory = &state.system.memory;
        let memory_percent = crate::core::format::percent(memory.used_bytes, memory.total_bytes);
        let line = format!(
            "tick {} cpu {:.1}% mem {:.1}% processes {}",
            state.tick,
            state.system.cpu.total_percent,
            memory_percent,
            state.processes.len()
        );
        println!("{line}");
        if let Some(path) = &log_file {
            append_log(path, &line);
        }
    });
    sampler.flush_history();
    println!("sentinel daemon: stopped");
    Ok(())
}

/// Add a line to the daemon log.
fn append_log(path: &std::path::Path, line: &str) {
    use std::io::Write;
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(
            file,
            "[{}] {line}",
            crate::view::clock(crate::collect::now_millis() / 1_000)
        );
    }
}

/// Detach from the terminal and carry on without it.
fn detach() -> Result<(), String> {
    unsafe {
        let pid = libc::fork();
        if pid < 0 {
            return Err("cannot detach from the terminal".into());
        }
        if pid > 0 {
            libc::_exit(0);
        }
        libc::setsid();
        let null = std::ffi::CString::new("/dev/null").unwrap();
        let fd = libc::open(null.as_ptr(), libc::O_RDWR);
        if fd >= 0 {
            libc::dup2(fd, 0);
            libc::dup2(fd, 1);
            libc::dup2(fd, 2);
            if fd > 2 {
                libc::close(fd);
            }
        }
    }
    Ok(())
}

/// Summarise what happened over a window.
fn report(config: Config, last: Option<String>, json: bool) -> Result<(), String> {
    let window = match last {
        Some(text) => parse_duration(&text).map_err(|error| format!("--last: {error}"))?,
        None => Duration::from_secs(600),
    };
    let samples = match history_store(&config) {
        Some(store) => store.read(),
        None => Vec::new(),
    };
    let alerts = match &config.alerts.log_file {
        Some(path) => read_alerts(path),
        None => Vec::new(),
    };
    let report = crate::report::Report::build(&samples, &alerts, window);
    if json {
        println!("{}", report.to_json());
    } else {
        print!("{}", view::report(&report));
    }
    Ok(())
}

/// The history file, when the tool is keeping one.
fn history_store(config: &Config) -> Option<crate::store::Store> {
    if !config.general.persist_history {
        return None;
    }
    let dir = config.general.data_dir.clone().or_else(default_data_dir)?;
    Some(crate::store::Store::new(crate::store::history_path(&dir)))
}

/// Where the tool keeps its data when nothing else is said.
fn default_data_dir() -> Option<std::path::PathBuf> {
    let base = match std::env::var_os("XDG_DATA_HOME") {
        Some(dir) if !dir.is_empty() => std::path::PathBuf::from(dir),
        _ => std::path::PathBuf::from(std::env::var_os("HOME")?).join(".local/share"),
    };
    Some(base.join("sentinel"))
}

/// A sink that prints the alerts and writes them to the log.
pub fn print_alerts(config: &Config) -> crate::sample::AlertSink {
    let log_file = config.alerts.log_file.clone();
    Box::new(move |alert| {
        println!("{}", view::alert_line(alert));
        if let Some(path) = &log_file
            && let Err(error) = append_alert(path, alert)
        {
            eprintln!("sentinel: {error}");
        }
    })
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
    let mut sampler = sampler(config, config.general.interval, print_alerts(config));
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
