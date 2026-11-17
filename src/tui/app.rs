//! The live dashboard: a system summary on top, a process table below.

use std::sync::Arc;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table, TableState};

use crate::action;
use crate::config::Config;
use crate::core::format::bytes_to_human;
use crate::core::metrics::SystemSample;
use crate::core::process::ProcessSample;
use crate::policy::PolicyAction;
use crate::sample::{SamplerState, Shared};
use crate::view::Sort;

/// What the action menu is showing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Menu {
    /// The list of actions.
    Open,
    /// Waiting for the new priority.
    Value(PolicyAction),
    /// Waiting for the user to say yes.
    Confirm(PolicyAction, Option<i32>),
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
    pub fn choose(&mut self, action: PolicyAction) {
        self.menu = Some(match action {
            PolicyAction::Renice => Menu::Value(action),
            other => Menu::Confirm(other, None),
        });
    }

    /// The action the menu is waiting to confirm.
    pub fn pending_action(&self) -> Option<(PolicyAction, Option<i32>)> {
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
            PolicyAction::Renice => match value {
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
        let Some(path) = &self.config.actions.log_path() else {
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

    /// An app showing a known system sample, for tests.
    #[cfg(test)]
    pub fn with_system(system: SystemSample) -> App {
        let state = SamplerState {
            tick: 1,
            system,
            processes: Vec::new(),
            alerts: Vec::new(),
            exits: Vec::new(),
            restart_loops: Vec::new(),
            proposals: Vec::new(),
        };
        App::new(Arc::new(Shared::default())).with_state(state)
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
        self.clamp_selection();
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
            self.clamp_selection();
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
        let sample = &self.snapshot().system;
        let top_height = cpu_height(sample).max(11);
        let panes = Layout::vertical([
            Constraint::Length(top_height),
            Constraint::Length(5),
            Constraint::Length(4),
            Constraint::Min(6),
            Constraint::Length(1),
        ])
        .split(area);
        let top = Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(panes[0]);
        let right = Layout::vertical([Constraint::Length(6), Constraint::Min(5)]).split(top[1]);
        self.draw_cpu(frame, top[0]);
        self.draw_memory(frame, right[0]);
        self.draw_network(frame, right[1]);
        self.draw_disk(frame, panes[1]);
        self.draw_alerts(frame, panes[2]);
        self.draw_processes(frame, panes[3]);
        self.draw_footer(frame, panes[4]);
    }

    /// The key hints, or the result of the last action.
    fn draw_footer(&self, frame: &mut Frame, area: Rect) {
        let text = if self.status.is_empty() {
            " q quit   t/m/i/N sort   f filter   / search   ↑↓ select   enter detail   a actions"
                .to_string()
        } else {
            format!(" {}", self.status)
        };
        frame.render_widget(Paragraph::new(text), area);
    }

    /// The recent alerts, with their severity.
    fn draw_alerts(&self, frame: &mut Frame, area: Rect) {
        let block = panel("Alerts");
        let state = self.snapshot();
        let exits = &state.exits;
        let alerts = &state.alerts;
        let loops = &state.restart_loops;
        if alerts.is_empty() && exits.is_empty() && loops.is_empty() {
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
        for loop_ in loops.iter().rev().take(1) {
            lines.push(Line::from(vec![
                Span::styled("[RESTART] ", Style::default().fg(Color::Red)),
                Span::raw(crate::view::restart_loop_line(loop_)),
            ]));
        }
        frame.render_widget(Paragraph::new(lines).block(block), area);
    }

    /// The action menu for the selected process.
    fn draw_menu(&self, frame: &mut Frame, area: Rect) {
        let block = panel("Actions");
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
                    PolicyAction::Kill,
                    PolicyAction::Suspend,
                    PolicyAction::Resume,
                    PolicyAction::Renice,
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
                lines.push(Line::from(" k/s/r/n: act   d: dry run   esc: close"));
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

    /// The CPU meters: the totals, the load and one meter per core.
    fn draw_cpu(&self, frame: &mut Frame, area: Rect) {
        let sample = &self.snapshot().system;
        let mut lines = vec![summary_line("CPU", &cpu_summary(sample))];
        lines.push(summary_line(
            "Load",
            &format!("{}   {}", load_summary(sample), temperature_summary(sample)),
        ));
        let cores = &sample.cpu.per_core_percent;
        let per_line = cores_per_line(cores.len());
        let rows = cores.len().div_ceil(per_line);
        let chunks = rows.min(MAX_CORE_ROWS - usize::from(rows > MAX_CORE_ROWS));
        for row in 0..chunks {
            let mut spans = Vec::new();
            let start = row * per_line;
            let end = ((row + 1) * per_line).min(cores.len());
            for (core, percent) in cores.iter().enumerate().take(end).skip(start) {
                if core > start {
                    spans.push(Span::raw(" "));
                }
                spans.push(Span::raw(format!("{core:>2} ")));
                spans.extend(meter_spans(*percent, if per_line == 1 { 24 } else { 5 }));
                spans.push(Span::raw(format!(" {percent:>3.0}%")));
            }
            lines.push(Line::from(spans));
        }
        if rows > MAX_CORE_ROWS {
            lines.push(Line::from(format!(
                " +{} more cores",
                cores.len() - chunks * per_line
            )));
        }
        frame.render_widget(Paragraph::new(lines).block(panel("CPU")), area);
    }

    fn draw_memory(&self, frame: &mut Frame, area: Rect) {
        let sample = &self.snapshot().system;
        let memory = &sample.memory;
        let percent = if memory.total_bytes == 0 {
            0.0
        } else {
            memory.used_bytes as f64 / memory.total_bytes as f64 * 100.0
        };
        let mut meter = vec![Span::styled(
            format!("{:<10}", "Used"),
            Style::default().fg(Color::Cyan),
        )];
        meter.extend(meter_spans(percent, 12));
        meter.push(Span::raw(format!(
            " {percent:.0}%  free {}",
            bytes_to_human(memory.available_bytes)
        )));
        let lines = vec![
            summary_line(
                "Memory",
                &format!(
                    "{} used / {}",
                    bytes_to_human(memory.used_bytes),
                    bytes_to_human(memory.total_bytes)
                ),
            ),
            Line::from(meter),
            summary_line(
                "Cached",
                &format!(
                    "{}   Buffers {}",
                    bytes_to_human(memory.cached_bytes),
                    bytes_to_human(memory.buffers_bytes)
                ),
            ),
            summary_line(
                "Swap",
                &format!(
                    "{} used / {}",
                    bytes_to_human(memory.swap_used_bytes),
                    bytes_to_human(memory.swap_total_bytes)
                ),
            ),
        ];
        frame.render_widget(Paragraph::new(lines).block(panel("Memory")), area);
    }

    fn draw_disk(&self, frame: &mut Frame, area: Rect) {
        let sample = &self.snapshot().system;
        let mut lines = vec![summary_line("Disk", &disk_summary(sample))];
        if !sample.disk.devices.is_empty() {
            let devices = sample
                .disk
                .devices
                .iter()
                .map(|device| {
                    format!(
                        "{} r {} w {}",
                        device.device,
                        bytes_to_human(device.read_bytes_per_sec),
                        bytes_to_human(device.write_bytes_per_sec)
                    )
                })
                .collect::<Vec<_>>()
                .join("  ");
            lines.push(summary_line("Devices", &devices));
        }
        if !sample.disk.filesystems.is_empty() {
            let filesystems = sample
                .disk
                .filesystems
                .iter()
                .map(filesystem_summary)
                .collect::<Vec<_>>()
                .join("  ");
            lines.push(summary_line("Mounts", &filesystems));
        }
        frame.render_widget(Paragraph::new(lines).block(panel("Disk")), area);
    }

    fn draw_network(&self, frame: &mut Frame, area: Rect) {
        let sample = &self.snapshot().system;
        let mut lines = vec![summary_line("Network", &network_summary(sample))];
        for interface in &sample.network.interfaces {
            lines.push(summary_line(
                &interface.name,
                &format!(
                    "received {}  sent {}",
                    bytes_to_human(interface.rx_bytes_per_sec),
                    bytes_to_human(interface.tx_bytes_per_sec)
                ),
            ));
        }
        frame.render_widget(Paragraph::new(lines).block(panel("Network")), area);
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
            .map(|process| {
                Row::new(vec![
                    Cell::from(process.pid.to_string()),
                    Cell::from(process.user.clone()),
                    Cell::from(format!("{:.1}", process.cpu_percent)),
                    Cell::from(format!("{:.1}", process.mem_percent)),
                    Cell::from(crate::view::marked_name(
                        process,
                        &self.config.critical_processes.names,
                    )),
                ])
            })
            .collect();
        let title = format!("Processes ({}) | {}", processes.len(), self.status_line());
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
        .block(panel(&title));
        // The table scrolls itself to keep the selected row on screen.
        let selected = (self.selected < processes.len()).then_some(self.selected);
        let mut state = TableState::new().with_selected(selected);
        frame.render_stateful_widget(table, table_area, &mut state);
        if let Some(side_area) = side_area {
            if self.menu.is_some() {
                self.draw_menu(frame, side_area);
            } else {
                self.draw_detail(frame, side_area);
            }
        }
    }

    /// The detail of the selected process.
    fn draw_detail(&self, frame: &mut Frame, area: Rect) {
        let block = panel("Detail");
        let Some(process) = self.selected_process() else {
            frame.render_widget(Paragraph::new("no process selected").block(block), area);
            return;
        };
        let lines: Vec<Line> = vec![
            Line::from(format!("{} ({})", process.name, process.pid)),
            Line::from(format!(
                "parent {}  user {}  started {}",
                process.ppid,
                process.user,
                crate::view::clock(process.started_at)
            )),
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
        frame.render_widget(Paragraph::new(lines).block(block), area);
    }

    /// The process the table has selected.
    pub fn selected_process(&self) -> Option<ProcessSample> {
        self.rows().get(self.selected).cloned()
    }

    /// Keep the selection on a row after the list shrinks.
    pub fn clamp_selection(&mut self) {
        let last = self.rows().len().saturating_sub(1);
        self.selected = self.selected.min(last);
    }
}

/// The most lines of core meters the CPU box shows before it summarises the rest.
const MAX_CORE_ROWS: usize = 8;

/// A boxed pane with a coloured title.
fn panel(title: &str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(Color::Cyan),
        ))
}

/// How tall the CPU box needs to be for its core meters.
fn cpu_height(sample: &SystemSample) -> u16 {
    let cores = sample.cpu.per_core_percent.len();
    let rows = cores.div_ceil(cores_per_line(cores));
    4 + rows.min(MAX_CORE_ROWS) as u16
}

/// One core per line while they fit; three with a gap per line on machines with many.
fn cores_per_line(cores: usize) -> usize {
    if cores > MAX_CORE_ROWS { 3 } else { 1 }
}

/// A small bar, green when idle and red when busy.
fn meter_spans(percent: f64, width: usize) -> Vec<Span<'static>> {
    let filled = ((percent.clamp(0.0, 100.0) / 100.0) * width as f64).round() as usize;
    let colour = match percent {
        percent if percent >= 80.0 => Color::Red,
        percent if percent >= 50.0 => Color::Yellow,
        _ => Color::Green,
    };
    let frame = Style::default().fg(Color::DarkGray);
    vec![
        Span::styled("[", frame),
        Span::styled("█".repeat(filled), Style::default().fg(colour)),
        Span::styled("░".repeat(width - filled), frame),
        Span::styled("]", frame),
    ]
}

fn filesystem_summary(filesystem: &crate::core::metrics::FilesystemSample) -> String {
    let percent = if filesystem.total_bytes == 0 {
        0.0
    } else {
        filesystem.used_bytes as f64 / filesystem.total_bytes as f64 * 100.0
    };
    format!(
        "{} {percent:.0}% of {}",
        filesystem.mount_point,
        bytes_to_human(filesystem.total_bytes)
    )
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
        Span::styled(format!("{label:<10}"), Style::default().fg(Color::Cyan)),
        Span::raw(value.to_string()),
    ])
}

fn cpu_summary(sample: &SystemSample) -> String {
    format!(
        "{:.1}%  user {:.1}%  sys {:.1}%  iowait {:.1}%",
        sample.cpu.total_percent,
        sample.cpu.user_percent,
        sample.cpu.system_percent,
        sample.cpu.iowait_percent
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

/// The dashboard, rendered as plain text.
///
/// Used by the tests and by the `dashboard` example, which is how the README
/// picture is made.
pub fn render(app: &App, width: u16, height: u16) -> String {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        App::new(Arc::new(Shared::default()))
    }

    #[test]
    fn a_meter_is_framed_and_filled_by_percentage() {
        let bar: String = meter_spans(50.0, 4)
            .iter()
            .map(|span| span.content.as_ref())
            .collect();

        assert_eq!(bar, "[██░░]");
    }

    #[test]
    fn the_boxes_show_per_core_disk_and_network_detail() {
        let sample = SystemSample {
            cpu: crate::core::metrics::CpuSample {
                per_core_percent: vec![10.0, 20.0, 30.0, 40.0],
                ..Default::default()
            },
            disk: crate::core::metrics::DiskSample {
                devices: vec![crate::core::metrics::DiskDeviceSample {
                    device: "sda".into(),
                    ..Default::default()
                }],
                filesystems: vec![crate::core::metrics::FilesystemSample {
                    mount_point: "/".into(),
                    used_bytes: 50,
                    total_bytes: 100,
                    available_bytes: 50,
                }],
                ..Default::default()
            },
            network: crate::core::metrics::NetworkSample {
                interfaces: vec![crate::core::metrics::InterfaceSample {
                    name: "eth0".into(),
                    ..Default::default()
                }],
                ..Default::default()
            },
            ..Default::default()
        };
        let app = App::with_system(sample);

        let text = render(&app, 120, 40);

        assert!(text.contains("20%"), "{text}");
        assert!(text.contains("40%"), "{text}");
        assert!(text.contains("sda"), "{text}");
        assert!(text.contains("/ 50% of 100 B"), "{text}");
        assert!(text.contains("eth0"), "{text}");
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
        app.choose(PolicyAction::Kill);

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
        app.choose(PolicyAction::Kill);
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

        for title in ["CPU", "Memory", "Disk", "Network", "Alerts", "Processes"] {
            assert!(text.contains(title), "no {title} box:\n{text}");
        }
        assert!(text.contains("PID"), "{text}");
        assert!(text.contains("q quit"), "{text}");
    }
}
