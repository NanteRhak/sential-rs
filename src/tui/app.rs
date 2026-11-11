//! The live dashboard: a system summary on top, a process table below.

use std::sync::{Arc, RwLock};

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use crate::core::format::bytes_to_human;
use crate::core::metrics::SystemSample;
use crate::core::process::ProcessSample;
use crate::sample::SamplerState;
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
    state: Arc<RwLock<Arc<SamplerState>>>,
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
}

impl App {
    pub fn new(state: Arc<RwLock<Arc<SamplerState>>>) -> Self {
        Self {
            state,
            sort: Sort::Cpu,
            selected: 0,
            should_quit: false,
            status: String::new(),
            filter: None,
            search: None,
            typing: None,
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
        App::new(Arc::new(RwLock::new(Arc::new(state))))
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
        self.state
            .read()
            .map(|state| state.clone())
            .unwrap_or_default()
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
        let panes = Layout::vertical([Constraint::Length(9), Constraint::Min(4)]).split(area);
        self.draw_summary(frame, panes[0]);
        self.draw_processes(frame, panes[1]);
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
        frame.render_widget(table, area);
    }
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
        let state = Arc::new(RwLock::new(Arc::new(SamplerState::default())));
        App::new(state)
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
    fn both_panes_are_on_screen() {
        let text = render(&app(), 100, 30);

        assert!(text.contains("System"), "{text}");
        assert!(text.contains("CPU"), "{text}");
        assert!(text.contains("Memory"), "{text}");
        assert!(text.contains("Processes"), "{text}");
        assert!(text.contains("PID"), "{text}");
    }
}
