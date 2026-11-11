//! What each subcommand does.

use std::time::Duration;

use crate::cli::Command;
use crate::collect::proc::ProcCollector;
use crate::config::{Config, parse_duration};
use crate::sample::Sampler;
use crate::view;

/// Run one command. `Err` is a message for the user and a non-zero exit.
pub fn run(command: Command, config: Config) -> Result<(), String> {
    match command {
        Command::Health { interval, loopback } => health(config, interval, loopback),
        Command::Ps => ps(config),
        Command::Top => Err("top is not implemented yet".into()),
        Command::Tui => Err("tui is not implemented yet".into()),
        Command::Watch => Err("watch is not implemented yet".into()),
        Command::Alerts => Err("alerts is not implemented yet".into()),
        Command::Inspect => Err("inspect is not implemented yet".into()),
        Command::Kill => Err("kill is not implemented yet".into()),
        Command::Suspend => Err("suspend is not implemented yet".into()),
        Command::Resume => Err("resume is not implemented yet".into()),
        Command::Renice => Err("renice is not implemented yet".into()),
        Command::Report => Err("report is not implemented yet".into()),
        Command::Daemon => Err("daemon is not implemented yet".into()),
    }
}

fn health(config: Config, interval: Option<String>, loopback: bool) -> Result<(), String> {
    let interval = resolve_interval(interval, config.general.interval)?;
    let collector = ProcCollector::new(crate::collect::proc::ProcFs::system(), loopback);
    let mut sampler = Sampler::new(interval, Box::new(collector));
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

fn ps(config: Config) -> Result<(), String> {
    let mut sampler = Sampler::new(config.general.interval, Box::new(ProcCollector::default()));
    let state = sampler.warm_sample();
    if let Some(error) = sampler.last_error() {
        return Err(format!("cannot read system metrics: {error}"));
    }
    print!("{}", view::processes(&state.processes));
    Ok(())
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
