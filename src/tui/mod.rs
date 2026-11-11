//! The live terminal dashboard.

pub mod app;

use std::io::{Stdout, stdout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

use crate::collect::proc::ProcCollector;
use crate::config::Config;
use crate::sample::{Sampler, SamplerState};

use app::App;

/// Run the dashboard until the user quits.
pub fn run(config: Config) -> Result<(), String> {
    let mut sampler = Sampler::new(config.general.interval, Box::new(ProcCollector::default()));
    let state = sampler.handle();
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    std::thread::spawn(move || sampler.run(&|| thread_stop.load(Ordering::Relaxed)));

    let mut terminal = setup()?;
    let guard = TerminalGuard;
    let result = event_loop(&mut terminal, App::new(state));
    drop(guard);
    stop.store(true, Ordering::Relaxed);
    result
}

/// Put the terminal into the state the dashboard needs.
fn setup() -> Result<Terminal<CrosstermBackend<Stdout>>, String> {
    enable_raw_mode().map_err(|error| error.to_string())?;
    execute!(stdout(), EnterAlternateScreen).map_err(|error| error.to_string())?;
    let terminal =
        Terminal::new(CrosstermBackend::new(stdout())).map_err(|error| error.to_string())?;
    Ok(terminal)
}

/// Puts the terminal back the way it was, however the dashboard ends.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen);
    }
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    mut app: App,
) -> Result<(), String> {
    loop {
        terminal
            .draw(|frame| app.draw(frame))
            .map_err(|error| error.to_string())?;
        if event::poll(Duration::from_millis(200)).map_err(|error| error.to_string())?
            && let Event::Key(key) = event::read().map_err(|error| error.to_string())?
            && key.kind == KeyEventKind::Press
        {
            on_key(&mut app, key);
        }
        if app.should_quit {
            return Ok(());
        }
    }
}

/// What each key does.
pub fn on_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }
        _ => {}
    }
}

/// The state the dashboard reads, exposed for tests.
pub type SharedState = Arc<RwLock<Arc<SamplerState>>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn q_quits() {
        let state = Arc::new(RwLock::new(Arc::new(SamplerState::default())));
        let mut app = App::new(state);

        on_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
        );

        assert!(app.should_quit);
    }

    #[test]
    fn other_keys_do_not_quit() {
        let state = Arc::new(RwLock::new(Arc::new(SamplerState::default())));
        let mut app = App::new(state);

        on_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
        );

        assert!(!app.should_quit);
    }
}
