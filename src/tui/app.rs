//! The live dashboard: a system summary on top, a process table below.

use std::sync::Arc;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Sparkline, Table};

use crate::action;
use crate::config::Config;
use crate::core::format::bytes_to_human;
use crate::core::metrics::SystemSample;
use crate::core::process::ProcessSample;
use crate::sample::{SamplerState, Shared};
use crate::view::Sort;

/// What the action menu can do to the selected process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    Kill,
    Suspend,
    Resume,
    Renice,
}

impl ActionKind {
    pub fn label(&self) -> &'static str {
        match self {
            ActionKind::Kill => "kill",
            ActionKind::Suspend => "suspend",
            ActionKind::Resume => "resume",
            ActionKind::Renice => "renice",
        }
    }

    pub fn detail(&self) -> &'static str {
        match self {
            ActionKind::Kill => "send SIGTERM",
            ActionKind::Suspend => "stop the process",
            ActionKind::Resume => "continue the process",
            ActionKind::Renice => "change the priority",
        }
    }

    /// The signal the action sends, if it sends one.
    pub fn signal(&self) -> Option<i32> {
        match self {
            ActionKind::Kill => Some(libc::SIGTERM),
            ActionKind::Suspend => Some(libc::SIGSTOP),
            ActionKind::Resume => Some(libc::SIGCONT),
            ActionKind::Renice => None,
        }
    }
}

/// What the action menu is showing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Menu {
    /// The list of actions.
    Open,
    /// Waiting for the new priority.
    Value(ActionKind),
    /// Waiting for the user to say yes.
    Confirm(ActionKind, Option<i32>),
}

/// The resource the table is narrowed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resource {
    Cpu,
    Memory,
    Io,
    Network,
}

impl Resource {
    pub fn label(&self) -> &'static str {
        match self {
            Resource::Cpu => "cpu",
            Resource::Memory => "memory",
            Resource::Io => "i/o",
            Resource::Network => "network",
        }
    }
}

/// What the dashboard is showing.
pub struct App {
    shared: Arc<Shared>,
    pub sort: Sort,
    pub selected: usize,
    pub should_quit: bool,
    /// A message shown along the bottom, such as what the last key did.
    pub status: String,
    /// The resource the table is narrowed to, if any.
    pub filter: Option<Resource>,
    /// The name being searched for, if any.
    pub search: Option<String>,
    /// The search being typed, before it is applied.
    pub typing: Option<String>,
    /// Whether the detail panel is open.
    pub detail: bool,
    /// The action menu, when it is open.
    pub menu: Option<Menu>,
    /// The configuration the actions follow.
    pub config: Config,
    /// The number being typed in the menu.
    value: String,
}

impl App {
    pub fn new(shared: Arc<Shared>) -> Self {
        Self {
            shared,
            sort: Sort::Cpu,
            selected: 0,
            should_quit: false,
            status: String::new(),
            filter: None,
            search: None,
            typing: None,
            detail: false,
            menu: None,
            config: Config::default(),
            value: String::new(),
        }
    }

    /// The number being typed in the menu.
    pub fn value_text(&self) -> &str {
        &self.value
    }

    /// Add a digit to the number being typed.
    pub fn push_value(&mut self, character: char) {
        self.value.push(character);
    }

    pub fn clear_value(&mut self) {
        self.value.clear();
    }

    /// Open the action menu for the selected process.
    pub fn open_menu(&mut self) {
        self.menu = Some(Menu::Open);
    }

    /// Close the action menu.
    pub fn close_menu(&mut self) {
        self.menu = None;
    }

    /// Choose an action from the menu.
    pub fn choose(&mut self, action: ActionKind) {
        self.menu = Some(match action {
            ActionKind::Renice => Menu::Value(action),
            other => Menu::Confirm(other, None),
        });
    }

    /// The action the menu is waiting to confirm.
    pub fn pending_action(&self) -> Option<(ActionKind, Option<i32>)> {
        match &self.menu {
            Some(Menu::Confirm(action, value)) => Some((*action, *value)),
            _ => None,
        }
    }

    /// Do the action the menu has chosen.
    ///
    /// A dry run reports what would happen and changes nothing.
    pub fn perform(&mut self, dry_run: bool) {
        let Some((action, value)) = self.pending_action() else {
            return;
        };
        let Some(process) = self.selected_process() else {
            self.status = "no process is selected".to_string();
            return;
        };
        let pid = process.pid;
        let name = process.name.clone();

        if let Some(rule) = action::protection(&name, &self.config.critical_processes.names) {
            self.status = format!("{name} is protected by the critical_processes rule ({rule})");
            self.record(
                "kill",
                pid,
                &name,
                "tui",
                &format!("refused: protected by {rule}"),
            );
            return;
        }

        if dry_run {
            self.status = format!("would {} {name} ({pid})", action.label());
            self.record(
                action.label(),
                pid,
                &name,
                "tui",
                "dry run: nothing was sent",
            );
            return;
        }

        let result = match action {
            ActionKind::Renice => match value {
                Some(value) => {
                    action::renice(pid, value).map(|()| format!("priority set to {value}"))
                }
                None => Err("no priority was given".to_string()),
            },
            other => match other.signal() {
                Some(signal) => action::send_signal(pid, signal)
                    .map(|()| format!("sent {}", action::signal_name(signal))),
                None => Err("that action sends no signal".to_string()),
            },
        };
        match result {
            Ok(outcome) => self.status = format!("{} {name} ({pid}): {outcome}", action.label()),
            Err(error) => {
                self.status = format!("{} {name} ({pid}) failed: {error}", action.label())
            }
        }
        self.record(action.label(), pid, &name, "tui", &self.status.clone());
    }

    /// Write an action to the action log, the same log the CLI writes to.
    fn record(&mut self, action: &str, pid: u32, process_name: &str, reason: &str, result: &str) {
        let Some(path) = &self.config.actions.log_file else {
            return;
        };
        let record = action::ActionRecord {
            timestamp: crate::collect::now_millis(),
            action: action.to_string(),
            pid,
            process_name: process_name.to_string(),
            reason: reason.to_string(),
            user_requested: true,
            result: result.to_string(),
        };
        if let Err(error) = action::write_record(path, &record) {
            self.status = format!("could not write the action log: {error}");
        }
    }

    /// An app showing a known set of processes, for tests.
    /// An app with a configuration, for tests.
    #[cfg(test)]
    pub fn with_config(mut self, config: Config) -> App {
        self.config = config;
        self
    }

    #[cfg(test)]
    pub fn with_processes(processes: Vec<ProcessSample>) -> App {
        let state = SamplerState {
            tick: 1,
            system: SystemSample::default(),
            processes,
            alerts: Vec::new(),
            exits: Vec::new(),
            restart_loops: Vec::new(),
            proposals: Vec::new(),
        };
        App::new(Arc::new(Shared::default())).with_state(state)
    }

    /// An app showing a known set of alerts, for tests.
    #[cfg(test)]
    pub fn with_alerts(alerts: Vec<crate::alert::Alert>) -> App {
        let shared = Arc::new(Shared::default());
        shared.publish(
            Arc::new(SamplerState {
                tick: 1,
                system: SystemSample::default(),
                processes: Vec::new(),
                alerts,
                exits: Vec::new(),
                restart_loops: Vec::new(),
                proposals: Vec::new(),
            }),
            std::time::Duration::from_secs(600),
        );
        App::new(shared)
    }

    /// An app showing a known set of samples, for tests.
    #[cfg(test)]
    pub fn with_history(history: Vec<SystemSample>) -> App {
        let shared = Arc::new(Shared::default());
        for (index, system) in history.into_iter().enumerate() {
            shared.publish(
                Arc::new(SamplerState {
                    tick: index as u64,
                    system,
                    processes: Vec::new(),
                    alerts: Vec::new(),
                    exits: Vec::new(),
                    restart_loops: Vec::new(),
                    proposals: Vec::new(),
                }),
                std::time::Duration::from_secs(600),
            );
        }
        App::new(shared)
    }

    #[cfg(test)]
    fn with_state(self, state: SamplerState) -> App {
        self.shared
            .publish(Arc::new(state), std::time::Duration::from_secs(600));
        self
    }

    /// Narrow the table to the next resource, and back to everything after
    /// the last one.
    pub fn cycle_filter(&mut self) {
        self.filter = match self.filter {
            None => Some(Resource::Cpu),
            Some(Resource::Cpu) => Some(Resource::Memory),
            Some(Resource::Memory) => Some(Resource::Io),
            Some(Resource::Io) => Some(Resource::Network),
            Some(Resource::Network) => None,
        };
    }

    /// Start typing a search.
    pub fn start_search(&mut self) {
        self.typing = Some(String::new());
    }

    pub fn type_char(&mut self, character: char) {
        if let Some(typing) = self.typing.as_mut() {
            typing.push(character);
        }
    }

    pub fn backspace(&mut self) {
        if let Some(typing) = self.typing.as_mut() {
            typing.pop();
        }
    }

    /// Apply the search being typed. An empty search shows everything again.
    pub fn apply_search(&mut self) {
        if let Some(typing) = self.typing.take() {
            self.search = if typing.is_empty() {
                None
            } else {
                Some(typing)
            };
        }
    }

    /// Stop typing without changing the search.
    pub fn cancel_search(&mut self) {
        self.typing = None;
    }

    /// The line above the table saying what is being shown.
    pub fn status_line(&self) -> String {
        let mut parts = vec![format!("sort: {}", sort_label(self.sort))];
        if let Some(filter) = self.filter {
            parts.push(format!("filter: {}", filter.label()));
        }
        match (&self.typing, &self.search) {
            (Some(typing), _) => parts.push(format!("/{typing}_")),
            (None, Some(search)) => parts.push(format!("/{search}")),
            (None, None) => {}
        }
        parts.join("  ")
    }

    /// The latest published sample. The lock is held for the length of the
    /// clone only, so a slow collection never freezes the display.
    pub fn snapshot(&self) -> Arc<SamplerState> {
        self.shared.state()
    }

    /// The recent samples, oldest first.
    pub fn history(&self) -> Vec<Arc<SamplerState>> {
        self.shared.history()
    }

    /// The rows the table shows: narrowed by the filter and the search, then
    /// ordered. The collection keeps running underneath; only the view changes.
    pub fn rows(&self) -> Vec<ProcessSample> {
        let mut processes = self.snapshot().processes.clone();
        if let Some(filter) = self.filter {
            processes.retain(|process| uses(process, filter));
        }
        if let Some(search) = &self.search {
            let needle = search.to_lowercase();
            processes.retain(|process| process.name.to_lowercase().contains(&needle));
        }
        crate::view::sort_processes(&mut processes, self.sort);
        processes
    }

    pub fn draw(&self, frame: &mut Frame) {
        let area = frame.area();
        let panes = Layout::vertical([
            Constraint::Length(8),
            Constraint::Length(4),
            Constraint::Length(8),
            Constraint::Min(4),
        ])
        .split(area);
        self.draw_summary(frame, panes[0]);
        self.draw_alerts(frame, panes[1]);
        self.draw_graphs(frame, panes[2]);
        self.draw_processes(frame, panes[3]);
    }

    /// The recent alerts, with their severity.
    fn draw_alerts(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title(" Alerts ");
        let state = self.snapshot();
        let exits = &state.exits;
        let alerts = &state.alerts;
        if alerts.is_empty() && exits.is_empty() {
            frame.render_widget(Paragraph::new("no alerts").block(block), area);
            return;
        }
        let mut lines: Vec<Line> = alerts
            .iter()
            .rev()
            .take(2)
            .map(|alert| {
                let colour = match alert.severity {
                    crate::core::Severity::Info => Color::Blue,
                    crate::core::Severity::Warning => Color::Yellow,
                    crate::core::Severity::Critical => Color::Red,
                };
                Line::from(vec![
                    Span::styled(
                        format!("[{:?}] ", alert.severity).to_uppercase(),
                        Style::default().fg(colour),
                    ),
                    Span::raw(alert.message.clone()),
                ])
            })
            .collect();
        for exit in exits.iter().rev().take(1) {
            lines.push(Line::from(vec![
                Span::styled("[EXIT] ", Style::default().fg(Color::Magenta)),
                Span::raw(crate::view::exit_line(exit)),
            ]));
        }
        frame.render_widget(Paragraph::new(lines).block(block), area);
    }

    /// The action menu for the selected process.
    fn draw_menu(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title(" Actions ");
        let Some(process) = self.selected_process() else {
            frame.render_widget(Paragraph::new("no process selected").block(block), area);
            return;
        };
        let mut lines = vec![Line::from(format!(
            "for {} ({})",
            process.name, process.pid
        ))];
        match self.menu {
            Some(Menu::Open) => {
                for (index, action) in [
                    ActionKind::Kill,
                    ActionKind::Suspend,
                    ActionKind::Resume,
                    ActionKind::Renice,
                ]
                .into_iter()
                .enumerate()
                {
                    lines.push(Line::from(format!(
                        " {}. {:<8} {}",
                        index + 1,
                        action.label(),
                        action.detail()
                    )));
                }
                lines.push(Line::from(" d: dry run   esc: close"));
            }
            Some(Menu::Value(action)) => {
                lines.push(Line::from(format!(
                    "new priority for {}: type a number and press enter",
                    action.label()
                )));
            }
            Some(Menu::Confirm(action, value)) => {
                lines.push(Line::from(format!(
                    "{} {} ({})?",
                    action.label(),
                    process.name,
                    process.pid
                )));
                if let Some(value) = value {
                    lines.push(Line::from(format!("new priority: {value}")));
                }
                lines.push(Line::from(" y: do it   d: dry run   n: cancel"));
            }
            None => {}
        }
        frame.render_widget(Paragraph::new(lines).block(block), area);
    }

    /// The usage graphs, drawn from the recent history.
    fn draw_graphs(&self, frame: &mut Frame, area: Rect) {
        let history = self.history();
        let graphs = [
            ("CPU %", cpu_series(&history), 100),
            ("Memory %", memory_series(&history), 100),
            ("Disk bytes/s", disk_series(&history), 0),
            ("Network bytes/s", network_series(&history), 0),
        ];
        let columns = Layout::horizontal([Constraint::Ratio(1, 4); 4]).split(area);
        for (index, (title, series, max)) in graphs.into_iter().enumerate() {
            let block = Block::default()
                .borders(Borders::ALL)
                .title(format!(" {title} "));
            if series.len() < 2 {
                frame.render_widget(Paragraph::new("no data").block(block), columns[index]);
                continue;
            }
            let peak = match max {
                0 => series.iter().copied().max().unwrap_or(1).max(1),
                fixed => fixed,
            };
            let sparkline = Sparkline::default()
                .data(&series)
                .max(peak)
                .style(Style::default().fg(Color::Green));
            frame.render_widget(sparkline.block(block), columns[index]);
        }
    }

    fn draw_summary(&self, frame: &mut Frame, area: Rect) {
        let sample = &self.snapshot().system;
        let mut lines = vec![summary_line("CPU", &cpu_summary(sample))];
        lines.push(summary_line("Memory", &memory_summary(sample)));
        lines.push(summary_line("Disk", &disk_summary(sample)));
        lines.push(summary_line("Network", &network_summary(sample)));
        lines.push(summary_line("Temperature", &temperature_summary(sample)));
        lines.push(summary_line("Load", &load_summary(sample)));
        let block = Block::default().borders(Borders::ALL).title(" System ");
        frame.render_widget(Paragraph::new(lines).block(block), area);
    }

    fn draw_processes(&self, frame: &mut Frame, area: Rect) {
        let (table_area, side_area) = if self.detail || self.menu.is_some() {
            let halves =
                Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)])
                    .split(area);
            (halves[0], Some(halves[1]))
        } else {
            (area, None)
        };
        let processes = self.rows();
        let header = Row::new(vec![
            Cell::from("PID"),
            Cell::from("USER"),
            Cell::from("CPU%"),
            Cell::from("MEM%"),
            Cell::from("NAME"),
        ]);
        let rows: Vec<Row> = processes
            .iter()
            .enumerate()
            .map(|(index, process)| {
                let style = if index == self.selected {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                };
                Row::new(vec![
                    Cell::from(process.pid.to_string()),
                    Cell::from(process.user.clone()),
                    Cell::from(format!("{:.1}", process.cpu_percent)),
                    Cell::from(format!("{:.1}", process.mem_percent)),
                    Cell::from(process.name.clone()),
                ])
                .style(style)
            })
            .collect();
        let title = format!(" Processes ({}) | {} ", processes.len(), self.status_line());
        let table = Table::new(
            rows,
            [
                Constraint::Length(8),
                Constraint::Length(12),
                Constraint::Length(7),
                Constraint::Length(7),
                Constraint::Min(10),
            ],
        )
        .header(header)
        .row_highlight_style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL).title(title));
        frame.render_widget(table, table_area);
        if let Some(side_area) = side_area {
            if self.menu.is_some() {
                self.draw_menu(frame, side_area);
            } else {
                self.draw_detail(frame, side_area);
            }
        }
    }

    /// The detail of the selected process, with its recent usage.
    fn draw_detail(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title(" Detail ");
        let Some(process) = self.selected_process() else {
            frame.render_widget(Paragraph::new("no process selected").block(block), area);
            return;
        };
        let mut lines: Vec<Line> = vec![
            Line::from(format!("{} ({})", process.name, process.pid)),
            Line::from(format!(
                "cmd {}",
                if process.cmd.is_empty() {
                    "not readable"
                } else {
                    &process.cmd
                }
            )),
            Line::from(format!(
                "cpu {:.1}%  memory {} ({:.1}%)",
                process.cpu_percent,
                bytes_to_human(process.mem_rss_bytes),
                process.mem_percent
            )),
            Line::from(format!(
                "threads {}  fds {}  nice {}",
                process.thread_count, process.fd_count, process.nice
            )),
            Line::from(format!(
                "cgroup {}",
                process.cgroup.as_deref().unwrap_or("not readable")
            )),
        ];
        let series = self.process_series(process.pid);
        if series.len() >= 2 {
            let graph = Layout::vertical([Constraint::Min(3), Constraint::Length(3)]).split(area);
            frame.render_widget(Paragraph::new(lines).block(block), graph[0]);
            let sparkline = Sparkline::default()
                .data(&series)
                .max(100)
                .style(Style::default().fg(Color::Green));
            frame.render_widget(
                sparkline.block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Recent CPU % "),
                ),
                graph[1],
            );
            return;
        }
        lines.push(Line::from("no usage history yet"));
        frame.render_widget(Paragraph::new(lines).block(block), area);
    }

    /// The process the table has selected.
    pub fn selected_process(&self) -> Option<ProcessSample> {
        self.rows().get(self.selected).cloned()
    }

    /// The CPU usage of a process over the recent history.
    pub fn process_series(&self, pid: u32) -> Vec<u64> {
        self.history()
            .iter()
            .filter_map(|state| {
                state
                    .processes
                    .iter()
                    .find(|process| process.pid == pid)
                    .map(|process| process.cpu_percent.round() as u64)
            })
            .collect()
    }
}

/// The CPU percentage of each sample in the history.
pub fn cpu_series(history: &[Arc<SamplerState>]) -> Vec<u64> {
    history
        .iter()
        .map(|state| state.system.cpu.total_percent.round() as u64)
        .collect()
}

/// The memory percentage of each sample in the history.
pub fn memory_series(history: &[Arc<SamplerState>]) -> Vec<u64> {
    history
        .iter()
        .map(|state| {
            let memory = &state.system.memory;
            if memory.total_bytes == 0 {
                0
            } else {
                (memory.used_bytes as f64 / memory.total_bytes as f64 * 100.0).round() as u64
            }
        })
        .collect()
}

/// The disk throughput of each sample in the history.
pub fn disk_series(history: &[Arc<SamplerState>]) -> Vec<u64> {
    history
        .iter()
        .map(|state| state.system.disk.read_bytes_per_sec + state.system.disk.write_bytes_per_sec)
        .collect()
}

/// The network throughput of each sample in the history.
pub fn network_series(history: &[Arc<SamplerState>]) -> Vec<u64> {
    history
        .iter()
        .map(|state| state.system.network.rx_bytes_per_sec + state.system.network.tx_bytes_per_sec)
        .collect()
}

/// Whether a process is using the resource the table is narrowed to.
fn uses(process: &ProcessSample, resource: Resource) -> bool {
    match resource {
        Resource::Cpu => process.cpu_percent > 0.0,
        Resource::Memory => process.mem_rss_bytes > 0,
        Resource::Io => crate::view::io_bytes(process) > 0,
        Resource::Network => crate::view::net_bytes(process) > 0,
    }
}

/// The name of a sort order.
pub fn sort_label(sort: Sort) -> &'static str {
    match sort {
        Sort::Cpu => "cpu",
        Sort::Mem => "memory",
        Sort::Name => "name",
        Sort::Io => "i/o",
        Sort::Net => "network",
        Sort::Pressure => "pressure",
    }
}

fn summary_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<14}"), Style::default().fg(Color::Cyan)),
        Span::raw(value.to_string()),
    ])
}

fn cpu_summary(sample: &SystemSample) -> String {
    format!(
        "{:.1}%  user {:.1}%  system {:.1}%  iowait {:.1}%  idle {:.1}%",
        sample.cpu.total_percent,
        sample.cpu.user_percent,
        sample.cpu.system_percent,
        sample.cpu.iowait_percent,
        sample.cpu.idle_percent
    )
}

fn memory_summary(sample: &SystemSample) -> String {
    format!(
        "{} used of {}, {} available, {} swap",
        bytes_to_human(sample.memory.used_bytes),
        bytes_to_human(sample.memory.total_bytes),
        bytes_to_human(sample.memory.available_bytes),
        bytes_to_human(sample.memory.swap_used_bytes)
    )
}

fn disk_summary(sample: &SystemSample) -> String {
    format!(
        "read {}  write {}",
        bytes_to_human(sample.disk.read_bytes_per_sec),
        bytes_to_human(sample.disk.write_bytes_per_sec)
    )
}

fn network_summary(sample: &SystemSample) -> String {
    format!(
        "received {}  sent {}",
        bytes_to_human(sample.network.rx_bytes_per_sec),
        bytes_to_human(sample.network.tx_bytes_per_sec)
    )
}

fn temperature_summary(sample: &SystemSample) -> String {
    if sample.sensors.temperatures.is_empty() {
        return "no readable sensors".to_string();
    }
    sample
        .sensors
        .temperatures
        .iter()
        .map(|reading| format!("{} {:.1}°C", reading.label, reading.celsius))
        .collect::<Vec<_>>()
        .join("  ")
}

fn load_summary(sample: &SystemSample) -> String {
    format!(
        "{:.2} {:.2} {:.2}",
        sample.cpu.load_avg_1, sample.cpu.load_avg_5, sample.cpu.load_avg_15
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn app() -> App {
        App::new(Arc::new(Shared::default()))
    }

    /// The rendered screen, as plain text.
    pub fn render(app: &App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let mut text = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                text.push_str(buffer.cell((x, y)).map(|cell| cell.symbol()).unwrap_or(" "));
            }
            text.push('\n');
        }
        text
    }

    #[test]
    fn the_graph_pane_draws_the_recent_history() {
        let history: Vec<SystemSample> = (0..10)
            .map(|index| SystemSample {
                cpu: crate::core::metrics::CpuSample {
                    total_percent: index as f64 * 5.0,
                    ..Default::default()
                },
                ..Default::default()
            })
            .collect();
        let app = App::with_history(history);

        let text = render(&app, 120, 40);

        assert!(text.contains("CPU %"), "{text}");
        assert!(text.contains("Memory %"), "{text}");
        assert!(text.contains("Disk bytes/s"), "{text}");
        assert!(text.contains("Network bytes/s"), "{text}");
        assert!(!text.contains("no data"), "{text}");
    }

    #[test]
    fn the_graph_pane_says_no_data_before_there_is_a_history() {
        let app = App::with_processes(vec![]);

        let text = render(&app, 120, 40);

        assert!(text.contains("no data"), "{text}");
    }

    #[test]
    fn the_detail_panel_shows_the_command_line_of_the_selected_process() {
        let process = ProcessSample {
            pid: 42,
            name: "backup".into(),
            cmd: "/usr/bin/backup --full".into(),
            user: "root".into(),
            cpu_percent: 12.5,
            mem_rss_bytes: 1024 * 1024,
            mem_percent: 1.5,
            thread_count: 4,
            fd_count: 12,
            ..Default::default()
        };
        let mut app = App::with_processes(vec![process]);
        app.detail = true;

        let text = render(&app, 120, 40);

        assert!(text.contains("Detail"), "{text}");
        assert!(text.contains("/usr/bin/backup --full"), "{text}");
        assert!(text.contains("12.5"), "{text}");
        assert!(text.contains("threads 4"), "{text}");
    }

    #[test]
    fn a_process_that_cannot_be_read_shows_what_is_available() {
        let process = ProcessSample {
            pid: 42,
            name: "secret".into(),
            cmd: String::new(),
            user: "root".into(),
            ..Default::default()
        };
        let mut app = App::with_processes(vec![process]);
        app.detail = true;

        let text = render(&app, 120, 40);

        assert!(text.contains("not readable"), "{text}");
    }

    #[test]
    fn the_alert_pane_lists_the_recent_alerts_with_their_severity() {
        let alert = crate::alert::Alert {
            id: 1,
            timestamp: 0,
            severity: crate::core::Severity::Critical,
            category: crate::alert::AlertCategory::Cpu,
            message: "CPU has been busy".into(),
            metric: Some("cpu_percent".into()),
            pid: None,
            process_name: None,
            current_value: 95.0,
            threshold: Some(90.0),
            baseline: None,
            suggestion: "look around".into(),
        };
        let app = App::with_alerts(vec![alert]);

        let text = render(&app, 120, 40);

        assert!(text.contains("Alerts"), "{text}");
        assert!(text.contains("CRITICAL"), "{text}");
        assert!(text.contains("CPU has been busy"), "{text}");
    }

    #[test]
    fn the_action_menu_lists_the_actions_for_the_selected_process() {
        let process = ProcessSample {
            pid: 42,
            name: "backup".into(),
            ..Default::default()
        };
        let mut app = App::with_processes(vec![process]);
        app.open_menu();

        let text = render(&app, 120, 40);

        assert!(text.contains("Actions"), "{text}");
        assert!(text.contains("kill"), "{text}");
        assert!(text.contains("suspend"), "{text}");
        assert!(text.contains("resume"), "{text}");
        assert!(text.contains("renice"), "{text}");
        assert!(text.contains("backup"), "{text}");
    }

    #[test]
    fn the_action_menu_asks_before_it_acts() {
        let process = ProcessSample {
            pid: 42,
            name: "backup".into(),
            ..Default::default()
        };
        let mut app = App::with_processes(vec![process]);
        app.open_menu();
        app.choose(ActionKind::Kill);

        let text = render(&app, 120, 40);

        assert!(text.contains("kill backup"), "{text}");
        assert!(text.contains("y: do it"), "{text}");
        assert!(text.contains("d: dry run"), "{text}");
    }

    #[test]
    fn an_action_from_the_dashboard_is_written_to_the_action_log() {
        let dir = std::env::temp_dir().join(format!("sentinel-tui-action-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("actions.jsonl");
        let config = Config {
            actions: crate::config::Actions {
                log_file: Some(log.clone()),
                confirm: false,
            },
            ..Default::default()
        };
        let process = ProcessSample {
            pid: 42,
            name: "backup".into(),
            ..Default::default()
        };
        let mut app = App::with_processes(vec![process]).with_config(config);
        app.open_menu();
        app.choose(ActionKind::Kill);
        app.perform(true);

        let records = crate::action::read_records(&log);
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].action, "kill");
        assert_eq!(records[0].pid, 42);
        assert_eq!(records[0].process_name, "backup");
        assert!(records[0].result.contains("dry run"), "{:?}", records[0]);
    }

    #[test]
    fn both_panes_are_on_screen() {
        let text = render(&app(), 100, 30);

        assert!(text.contains("System"), "{text}");
        assert!(text.contains("CPU"), "{text}");
        assert!(text.contains("Memory"), "{text}");
        assert!(text.contains("Processes"), "{text}");
        assert!(text.contains("PID"), "{text}");
    }
}
