//! The live terminal dashboard.

pub mod app;

use std::io::{Stdout, stdout};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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
use crate::sample::Sampler;

use crate::view::Sort;

use app::{ActionKind, App, Menu};

/// Run the dashboard until the user quits.
pub fn run(config: Config) -> Result<(), String> {
    let mut sampler = Sampler::new(config.general.interval, Box::new(ProcCollector::default()))
        .with_history(config.general.history_duration);
    let state = sampler.handle();
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    std::thread::spawn(move || sampler.run(&|| thread_stop.load(Ordering::Relaxed)));

    let mut terminal = setup()?;
    let guard = TerminalGuard;
    let mut app = App::new(state);
    app.config = config;
    let result = event_loop(&mut terminal, app);
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
    if app.menu.is_some() {
        on_menu_key(app, key);
        return;
    }
    if app.typing.is_some() {
        match key.code {
            KeyCode::Char(character) => app.type_char(character),
            KeyCode::Backspace => app.backspace(),
            KeyCode::Enter => app.apply_search(),
            KeyCode::Esc => app.cancel_search(),
            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }
        KeyCode::Char('t') => app.sort = Sort::Cpu,
        KeyCode::Char('m') => app.sort = Sort::Mem,
        KeyCode::Char('i') => app.sort = Sort::Io,
        KeyCode::Char('N') => app.sort = Sort::Net,
        KeyCode::Char('f') => app.cycle_filter(),
        KeyCode::Char('/') => app.start_search(),
        KeyCode::Up => app.selected = app.selected.saturating_sub(1),
        KeyCode::Down => app.selected += 1,
        KeyCode::Enter => app.detail = !app.detail,
        KeyCode::Char('a') => app.open_menu(),
        _ => {}
    }
}

/// What each key does while the action menu is open.
fn on_menu_key(app: &mut App, key: KeyEvent) {
    match &app.menu {
        Some(Menu::Value(_)) => match key.code {
            KeyCode::Char(character) if character.is_ascii_digit() => {
                app.push_value(character);
            }
            KeyCode::Enter => {
                if let Some(Menu::Value(action)) = app.menu {
                    let value = app.value_text().parse::<i32>().ok();
                    app.clear_value();
                    app.menu = Some(Menu::Confirm(action, value));
                }
            }
            KeyCode::Esc => app.close_menu(),
            _ => {}
        },
        Some(Menu::Confirm(action, value)) => match key.code {
            KeyCode::Char('y') => {
                app.perform(false);
                app.close_menu();
            }
            KeyCode::Char('d') => {
                app.perform(true);
                app.close_menu();
            }
            KeyCode::Char('n') | KeyCode::Esc => app.close_menu(),
            _ => {
                let _ = (action, value);
            }
        },
        _ => match key.code {
            KeyCode::Char('1') => app.choose(ActionKind::Kill),
            KeyCode::Char('2') => app.choose(ActionKind::Suspend),
            KeyCode::Char('3') => app.choose(ActionKind::Resume),
            KeyCode::Char('4') => app.choose(ActionKind::Renice),
            KeyCode::Char('d') => {
                app.choose(ActionKind::Kill);
                app.perform(true);
                app.close_menu();
            }
            KeyCode::Esc => app.close_menu(),
            _ => {}
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::process::ProcessSample;
    use crate::sample::Shared;

    fn process(pid: u32, name: &str, cpu: f64, rss: u64, read: u64) -> ProcessSample {
        ProcessSample {
            pid,
            name: name.into(),
            cpu_percent: cpu,
            mem_rss_bytes: rss,
            read_bytes_per_sec: read,
            ..Default::default()
        }
    }

    fn app() -> App {
        App::with_processes(vec![
            process(1, "firefox", 5.0, 100, 0),
            process(2, "backup", 0.0, 900, 50),
            process(3, "idle", 0.0, 10, 0),
        ])
    }

    fn press(app: &mut App, character: char) {
        on_key(
            app,
            KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE),
        );
    }

    fn enter(app: &mut App) {
        on_key(app, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    }

    #[test]
    fn the_sort_keys_change_the_order() {
        let mut app = app();

        press(&mut app, 'm');

        assert_eq!(app.sort, Sort::Mem);
        assert_eq!(app.rows()[0].pid, 2);
        assert!(
            app.status_line().contains("sort: memory"),
            "{}",
            app.status_line()
        );

        press(&mut app, 't');

        assert_eq!(app.rows()[0].pid, 1);
        assert!(
            app.status_line().contains("sort: cpu"),
            "{}",
            app.status_line()
        );
    }

    #[test]
    fn the_filter_narrows_the_rows_and_comes_back() {
        let mut app = app();

        press(&mut app, 'f');
        assert_eq!(app.filter, Some(app::Resource::Cpu));
        assert_eq!(app.rows().len(), 1);
        assert!(
            app.status_line().contains("filter: cpu"),
            "{}",
            app.status_line()
        );

        for _ in 0..4 {
            press(&mut app, 'f');
        }
        assert_eq!(app.filter, None);
        assert_eq!(app.rows().len(), 3);
    }

    #[test]
    fn a_search_narrows_the_rows_by_name() {
        let mut app = app();

        press(&mut app, '/');
        assert!(app.status_line().contains('/'), "{}", app.status_line());
        press(&mut app, 'b');
        press(&mut app, 'a');
        enter(&mut app);

        assert_eq!(app.search.as_deref(), Some("ba"));
        assert_eq!(app.rows().len(), 1);
        assert_eq!(app.rows()[0].pid, 2);
    }

    #[test]
    fn clearing_the_search_shows_everything_again() {
        let mut app = app();

        press(&mut app, '/');
        press(&mut app, 'b');
        enter(&mut app);
        assert_eq!(app.rows().len(), 1);

        press(&mut app, '/');
        enter(&mut app);

        assert_eq!(app.search, None);
        assert_eq!(app.rows().len(), 3);
    }

    #[test]
    fn typing_a_search_does_not_quit_the_dashboard() {
        let mut app = app();

        press(&mut app, '/');
        press(&mut app, 'q');

        assert!(!app.should_quit);
        assert_eq!(app.typing.as_deref(), Some("q"));
    }

    #[test]
    fn q_quits() {
        let mut app = App::new(Arc::new(Shared::default()));

        on_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
        );

        assert!(app.should_quit);
    }

    #[test]
    fn other_keys_do_not_quit() {
        let mut app = App::new(Arc::new(Shared::default()));

        on_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
        );

        assert!(!app.should_quit);
    }
}
