//! The live dashboard: a system summary on top, a process table below.

use std::sync::Arc;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Sparkline, Table};

use crate::core::format::bytes_to_human;
use crate::core::metrics::SystemSample;
use crate::core::process::ProcessSample;
use crate::sample::{SamplerState, Shared};
use crate::view::Sort;

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
        }
    }

    /// An app showing a known set of processes, for tests.
    #[cfg(test)]
    pub fn with_processes(processes: Vec<ProcessSample>) -> App {
        let state = SamplerState {
            tick: 1,
            system: SystemSample::default(),
            processes,
        };
        App::new(Arc::new(Shared::default())).with_state(state)
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
            Constraint::Length(8),
            Constraint::Min(4),
        ])
        .split(area);
        self.draw_summary(frame, panes[0]);
        self.draw_graphs(frame, panes[1]);
        self.draw_processes(frame, panes[2]);
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
        let (table_area, detail_area) = if self.detail {
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
        if let Some(detail_area) = detail_area {
            self.draw_detail(frame, detail_area);
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
    fn both_panes_are_on_screen() {
        let text = render(&app(), 100, 30);

        assert!(text.contains("System"), "{text}");
        assert!(text.contains("CPU"), "{text}");
        assert!(text.contains("Memory"), "{text}");
        assert!(text.contains("Processes"), "{text}");
        assert!(text.contains("PID"), "{text}");
    }
}
