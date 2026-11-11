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

/// What the dashboard is showing.
pub struct App {
    state: Arc<RwLock<Arc<SamplerState>>>,
    pub sort: Sort,
    pub selected: usize,
    pub should_quit: bool,
    /// A message shown along the bottom, such as what the last key did.
    pub status: String,
}

impl App {
    pub fn new(state: Arc<RwLock<Arc<SamplerState>>>) -> Self {
        Self {
            state,
            sort: Sort::Cpu,
            selected: 0,
            should_quit: false,
            status: String::new(),
        }
    }

    /// The latest published sample. The lock is held for the length of the
    /// clone only, so a slow collection never freezes the display.
    pub fn snapshot(&self) -> Arc<SamplerState> {
        self.state
            .read()
            .map(|state| state.clone())
            .unwrap_or_default()
    }

    /// The rows the table shows, after sorting.
    pub fn rows(&self) -> Vec<ProcessSample> {
        let mut processes = self.snapshot().processes.clone();
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
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" Processes ({}) ", processes.len())),
        );
        frame.render_widget(table, area);
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
