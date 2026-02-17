use std::collections::{HashMap, VecDeque};
use std::io::{self, BufRead, BufReader, Seek, SeekFrom};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode};
use crossterm::{execute};
use macros_rs::string;

#[cfg(any(target_os = "linux", target_os = "macos"))]
use pmc::process::{MemoryInfo, unix::NativeProcess as NativeProcess};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use pmc::process::unix::{get_listening_ports, is_port_open};

use pmc::helpers;
use pmc::process::{Process, Runner, get_process_cpu_usage_percentage};

use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Sparkline, Wrap};
use ratatui::Terminal;

const HISTORY_LEN: usize = 60;
const MAX_LOG_LINES: usize = 500;
const TICK_RATE: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Overview,
    Logs,
    InitialLogs,
}

#[derive(Clone, Copy, PartialEq)]
enum LogStream {
    Stdout,
    Stderr,
}

struct DashboardState {
    processes: Vec<(usize, Process)>,
    selected: usize,
    tab: Tab,
    log_stream: LogStream,
    log_lines: Vec<String>,
    log_scroll: usize,
    initial_out_lines: Vec<String>,
    initial_err_lines: Vec<String>,
    cpu_history: HashMap<usize, VecDeque<u64>>,
    mem_history: HashMap<usize, VecDeque<u64>>,
    port_map: HashMap<i64, Vec<u16>>,
    should_quit: bool,
}

impl DashboardState {
    fn new() -> Self {
        let mut state = DashboardState {
            processes: Vec::new(),
            selected: 0,
            tab: Tab::Overview,
            log_stream: LogStream::Stdout,
            log_lines: Vec::new(),
            log_scroll: 0,
            initial_out_lines: Vec::new(),
            initial_err_lines: Vec::new(),
            cpu_history: HashMap::new(),
            mem_history: HashMap::new(),
            port_map: HashMap::new(),
            should_quit: false,
        };
        state.refresh_processes();
        state.refresh_logs();
        state
    }

    fn refresh_processes(&mut self) {
        let runner = Runner::new();
        self.processes = runner.list.into_iter().collect();

        if self.selected >= self.processes.len() && !self.processes.is_empty() {
            self.selected = self.processes.len() - 1;
        }

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            self.port_map = get_listening_ports();
        }

        for (id, proc) in &self.processes {
            if !self.cpu_history.contains_key(id) {
                self.cpu_history.insert(*id, VecDeque::with_capacity(HISTORY_LEN));
            }
            if !self.mem_history.contains_key(id) {
                self.mem_history.insert(*id, VecDeque::with_capacity(HISTORY_LEN));
            }

            let mut cpu: f64 = 0.0;
            let mut mem: u64 = 0;

            if proc.running {
                if let Ok(native) = NativeProcess::new(proc.pid as u32) {
                    cpu = get_process_cpu_usage_percentage(proc.pid);
                    if let Ok(mi) = native.memory_info() {
                        mem = MemoryInfo::from(mi).rss;
                    }
                }
            }

            let cpu_buf = self.cpu_history.get_mut(id).unwrap();
            if cpu_buf.len() >= HISTORY_LEN {
                cpu_buf.pop_front();
            }
            cpu_buf.push_back((cpu * 100.0) as u64);

            let mem_buf = self.mem_history.get_mut(id).unwrap();
            if mem_buf.len() >= HISTORY_LEN {
                mem_buf.pop_front();
            }
            mem_buf.push_back(mem / 1024);
        }
    }

    fn refresh_logs(&mut self) {
        if self.processes.is_empty() {
            self.log_lines.clear();
            return;
        }

        let (_, proc) = &self.processes[self.selected];
        let logs = proc.logs();
        let path = match self.log_stream {
            LogStream::Stdout => &logs.out,
            LogStream::Stderr => &logs.error,
        };

        self.log_lines.clear();

        if let Ok(file) = std::fs::File::open(path) {
            let reader = BufReader::new(file);
            let all_lines: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();
            let start = if all_lines.len() > MAX_LOG_LINES {
                all_lines.len() - MAX_LOG_LINES
            } else {
                0
            };
            self.log_lines = all_lines[start..].to_vec();
        }
    }

    fn refresh_initial_logs(&mut self) {
        self.initial_out_lines.clear();
        self.initial_err_lines.clear();

        if self.processes.is_empty() {
            return;
        }

        let (_, proc) = &self.processes[self.selected];
        let logs = proc.logs();
        let initial = &proc.initial_logs;

        if let Ok(mut f) = std::fs::File::open(&logs.out) {
            if f.seek(SeekFrom::Start(initial.start_pos_out)).is_ok() {
                let reader = BufReader::new(f);
                self.initial_out_lines = reader.lines().take(100).filter_map(|l| l.ok()).collect();
            }
        }

        if let Ok(mut f) = std::fs::File::open(&logs.error) {
            if f.seek(SeekFrom::Start(initial.start_pos_error)).is_ok() {
                let reader = BufReader::new(f);
                self.initial_err_lines = reader.lines().take(100).filter_map(|l| l.ok()).collect();
            }
        }
    }

    fn selected_id(&self) -> Option<usize> {
        self.processes.get(self.selected).map(|(id, _)| *id)
    }

    fn do_restart(&mut self) {
        if let Some(id) = self.selected_id() {
            let mut runner = Runner::new();
            runner.restart(id, false);
        }
    }

    fn do_stop(&mut self) {
        if let Some(id) = self.selected_id() {
            let mut runner = Runner::new();
            runner.stop(id);
        }
    }

    fn do_start(&mut self) {
        if let Some(id) = self.selected_id() {
            let mut runner = Runner::new();
            runner.restart(id, false);
        }
    }

    fn do_flush(&mut self) {
        if let Some(id) = self.selected_id() {
            let mut runner = Runner::new();
            runner.flush(id);
            self.log_lines.clear();
            self.log_scroll = 0;
        }
    }
}

pub fn run() {
    enable_raw_mode().expect("Failed to enable raw mode");
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).expect("Failed to enter alternate screen");

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).expect("Failed to create terminal");

    let mut state = DashboardState::new();
    let mut last_tick = Instant::now();

    loop {
        terminal
            .draw(|f| draw_ui(f, &state))
            .expect("Failed to draw");

        let timeout = TICK_RATE.saturating_sub(last_tick.elapsed());

        if event::poll(timeout).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        state.should_quit = true;
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        state.should_quit = true;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if state.selected > 0 {
                            state.selected -= 1;
                            state.log_scroll = 0;
                            state.refresh_logs();
                            state.refresh_initial_logs();
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if state.selected + 1 < state.processes.len() {
                            state.selected += 1;
                            state.log_scroll = 0;
                            state.refresh_logs();
                            state.refresh_initial_logs();
                        }
                    }
                    KeyCode::Tab => {
                        state.tab = match state.tab {
                            Tab::Overview => Tab::Logs,
                            Tab::Logs => Tab::InitialLogs,
                            Tab::InitialLogs => Tab::Overview,
                        };
                        state.log_scroll = 0;
                        if state.tab == Tab::Logs {
                            state.refresh_logs();
                        }
                        if state.tab == Tab::InitialLogs {
                            state.refresh_initial_logs();
                        }
                    }
                    KeyCode::Char('1') => {
                        if state.tab == Tab::Logs {
                            state.log_stream = LogStream::Stdout;
                            state.log_scroll = 0;
                            state.refresh_logs();
                        }
                    }
                    KeyCode::Char('2') => {
                        if state.tab == Tab::Logs {
                            state.log_stream = LogStream::Stderr;
                            state.log_scroll = 0;
                            state.refresh_logs();
                        }
                    }
                    KeyCode::PageUp => {
                        state.log_scroll = state.log_scroll.saturating_add(10);
                    }
                    KeyCode::PageDown => {
                        state.log_scroll = state.log_scroll.saturating_sub(10);
                    }
                    KeyCode::Char('r') => {
                        state.do_restart();
                    }
                    KeyCode::Char('s') => {
                        state.do_stop();
                    }
                    KeyCode::Char('S') => {
                        state.do_start();
                    }
                    KeyCode::Char('f') => {
                        state.do_flush();
                    }
                    _ => {}
                }
            }
        }

        if last_tick.elapsed() >= TICK_RATE {
            state.refresh_processes();
            if state.tab == Tab::Logs {
                state.refresh_logs();
            }
            if state.tab == Tab::InitialLogs {
                state.refresh_initial_logs();
            }
            last_tick = Instant::now();
        }

        if state.should_quit {
            break;
        }
    }

    disable_raw_mode().expect("Failed to disable raw mode");
    execute!(terminal.backend_mut(), LeaveAlternateScreen).expect("Failed to leave alternate screen");
    terminal.show_cursor().expect("Failed to show cursor");
}

fn draw_ui(f: &mut ratatui::Frame, state: &DashboardState) {
    let size = f.area();

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(10),
            Constraint::Length(1),
        ])
        .split(size);

    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(35),
            Constraint::Percentage(65),
        ])
        .split(main_chunks[0]);

    draw_process_list(f, state, body_chunks[0]);

    match state.tab {
        Tab::Overview => draw_overview(f, state, body_chunks[1]),
        Tab::Logs => draw_logs(f, state, body_chunks[1]),
        Tab::InitialLogs => draw_initial_logs(f, state, body_chunks[1]),
    }

    draw_status_bar(f, state, main_chunks[1]);
}

fn draw_process_list(f: &mut ratatui::Frame, state: &DashboardState, area: Rect) {
    let items: Vec<ListItem> = state
        .processes
        .iter()
        .enumerate()
        .map(|(i, (id, proc))| {
            let status = if proc.running {
                Span::styled("online ", Style::default().fg(Color::Green))
            } else if proc.crash.crashed {
                Span::styled("crashed", Style::default().fg(Color::Red))
            } else {
                Span::styled("stopped", Style::default().fg(Color::Red))
            };

            let prefix = if i == state.selected { "> " } else { "  " };
            let style = if i == state.selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let ports: Vec<u16> = state
                .port_map
                .get(&proc.pid)
                .cloned()
                .unwrap_or_default();

            let port_spans: Vec<Span> = if !proc.running || ports.is_empty() {
                vec![]
            } else {
                let mut spans = vec![Span::styled(" :", Style::default().fg(Color::DarkGray))];
                for (idx, port) in ports.iter().enumerate() {
                    if idx > 0 {
                        spans.push(Span::styled(",", Style::default().fg(Color::DarkGray)));
                    }
                    let color = if is_port_open(*port) { Color::Green } else { Color::Red };
                    spans.push(Span::styled(port.to_string(), Style::default().fg(color)));
                }
                spans
            };

            let mut line_spans = vec![
                Span::styled(format!("{prefix}[{id}] "), style),
                Span::styled(
                    format!("{:<15} ", truncate_str(&proc.name, 15)),
                    style,
                ),
                status,
            ];
            line_spans.extend(port_spans);

            let line = Line::from(line_spans);

            ListItem::new(line)
        })
        .collect();

    let block = Block::default()
        .title(" Processes ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

fn draw_overview(f: &mut ratatui::Frame, state: &DashboardState, area: Rect) {
    if state.processes.is_empty() {
        let block = Block::default()
            .title(" Overview ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));
        let p = Paragraph::new("No processes found").block(block);
        f.render_widget(p, area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Min(4),
        ])
        .split(area);

    let (id, proc) = &state.processes[state.selected];

    // CPU sparkline
    let cpu_data: Vec<u64> = state
        .cpu_history
        .get(id)
        .map(|d| d.iter().copied().collect())
        .unwrap_or_default();

    let cpu_block = Block::default()
        .title(format!(" CPU % — {} ", proc.name))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));

    let cpu_sparkline = Sparkline::default()
        .block(cpu_block)
        .data(&cpu_data)
        .max(10000)
        .style(Style::default().fg(Color::Green))
        .bar_set(symbols::bar::NINE_LEVELS);

    f.render_widget(cpu_sparkline, chunks[0]);

    // Memory sparkline
    let mem_data: Vec<u64> = state
        .mem_history
        .get(id)
        .map(|d| d.iter().copied().collect())
        .unwrap_or_default();

    let mem_max = mem_data.iter().copied().max().unwrap_or(1024).max(1024);

    let mem_block = Block::default()
        .title(format!(" Memory — {} ", proc.name))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));

    let mem_sparkline = Sparkline::default()
        .block(mem_block)
        .data(&mem_data)
        .max(mem_max + mem_max / 4)
        .style(Style::default().fg(Color::Magenta))
        .bar_set(symbols::bar::NINE_LEVELS);

    f.render_widget(mem_sparkline, chunks[1]);

    // Info panel
    let pid_str = if proc.running {
        format!("{}", proc.pid)
    } else {
        string!("n/a")
    };

    let uptime_str = if proc.running {
        helpers::format_duration(proc.started)
    } else {
        string!("none")
    };

    let status_str = if proc.running {
        "online"
    } else if proc.crash.crashed {
        "crashed"
    } else {
        "stopped"
    };

    let mut cpu_val = string!("0.00%");
    let mut mem_val = string!("0b");

    if proc.running {
        if let Ok(native) = NativeProcess::new(proc.pid as u32) {
            cpu_val = format!("{:.2}%", get_process_cpu_usage_percentage(proc.pid));
            if let Ok(mi) = native.memory_info() {
                mem_val = helpers::format_memory(MemoryInfo::from(mi).rss);
            }
        }
    }

    let ports: Vec<u16> = state
        .port_map
        .get(&proc.pid)
        .cloned()
        .unwrap_or_default();

    let mut ports_spans = vec![Span::styled("Ports: ", Style::default().fg(Color::Cyan))];
    if !proc.running || ports.is_empty() {
        ports_spans.push(Span::styled("-", Style::default().fg(Color::DarkGray)));
    } else {
        for (idx, port) in ports.iter().enumerate() {
            if idx > 0 {
                ports_spans.push(Span::styled(", ", Style::default().fg(Color::DarkGray)));
            }
            let color = if is_port_open(*port) { Color::Green } else { Color::Red };
            ports_spans.push(Span::styled(port.to_string(), Style::default().fg(color)));
        }
    }

    let info_text = vec![
        Line::from(vec![
            Span::styled("Status: ", Style::default().fg(Color::Cyan)),
            Span::styled(
                status_str,
                Style::default().fg(if proc.running {
                    Color::Green
                } else {
                    Color::Red
                }),
            ),
            Span::raw("  "),
            Span::styled("PID: ", Style::default().fg(Color::Cyan)),
            Span::raw(&pid_str),
            Span::raw("  "),
            Span::styled("Uptime: ", Style::default().fg(Color::Cyan)),
            Span::raw(&uptime_str),
        ]),
        Line::from(vec![
            Span::styled("CPU: ", Style::default().fg(Color::Cyan)),
            Span::raw(&cpu_val),
            Span::raw("  "),
            Span::styled("Memory: ", Style::default().fg(Color::Cyan)),
            Span::raw(&mem_val),
            Span::raw("  "),
            Span::styled("Restarts: ", Style::default().fg(Color::Cyan)),
            Span::raw(format!("{}", proc.restarts)),
        ]),
        Line::from(ports_spans),
        Line::from(vec![
            Span::styled("Script: ", Style::default().fg(Color::Cyan)),
            Span::raw(truncate_str(&proc.script, 60)),
        ]),
        Line::from(vec![
            Span::styled("Path: ", Style::default().fg(Color::Cyan)),
            Span::raw(truncate_str(&proc.path.to_string_lossy(), 60)),
        ]),
    ];

    let info_block = Block::default()
        .title(format!(" Info — [{}] {} ", id, proc.name))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let info = Paragraph::new(info_text).block(info_block).wrap(Wrap { trim: false });
    f.render_widget(info, chunks[2]);
}

fn draw_logs(f: &mut ratatui::Frame, state: &DashboardState, area: Rect) {
    let stream_name = match state.log_stream {
        LogStream::Stdout => "stdout",
        LogStream::Stderr => "stderr",
    };

    let title = if let Some((id, proc)) = state.processes.get(state.selected) {
        format!(" Logs ({stream_name}) — [{}] {} ", id, proc.name)
    } else {
        format!(" Logs ({stream_name}) ")
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(match state.log_stream {
            LogStream::Stdout => Color::Green,
            LogStream::Stderr => Color::Red,
        }));

    if state.log_lines.is_empty() {
        let p = Paragraph::new("No logs available").block(block);
        f.render_widget(p, area);
        return;
    }

    let inner_height = area.height.saturating_sub(2) as usize;
    let total = state.log_lines.len();

    let max_scroll = if total > inner_height {
        total - inner_height
    } else {
        0
    };

    let scroll = state.log_scroll.min(max_scroll);
    let start = if total > inner_height {
        max_scroll - scroll
    } else {
        0
    };

    let visible_lines: Vec<Line> = state.log_lines[start..]
        .iter()
        .take(inner_height)
        .map(|l| {
            Line::from(Span::styled(
                l.clone(),
                Style::default().fg(match state.log_stream {
                    LogStream::Stdout => Color::White,
                    LogStream::Stderr => Color::LightRed,
                }),
            ))
        })
        .collect();

    let p = Paragraph::new(visible_lines).block(block);
    f.render_widget(p, area);
}

fn draw_initial_logs(f: &mut ratatui::Frame, state: &DashboardState, area: Rect) {
    let title = if let Some((id, proc)) = state.processes.get(state.selected) {
        format!(" Initial Logs — [{}] {} ", id, proc.name)
    } else {
        String::from(" Initial Logs ")
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    if state.processes.is_empty() {
        let p = Paragraph::new("No processes found").block(block);
        f.render_widget(p, area);
        return;
    }

    if state.initial_out_lines.is_empty() && state.initial_err_lines.is_empty() {
        let p = Paragraph::new("No initial logs captured yet").block(block);
        f.render_widget(p, area);
        return;
    }

    let inner_height = area.height.saturating_sub(2) as usize;
    let mut lines: Vec<Line> = Vec::new();

    if !state.initial_out_lines.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("─── stdout ({} lines):", state.initial_out_lines.len()),
            Style::default().fg(Color::DarkGray),
        )));
        for line in &state.initial_out_lines {
            lines.push(Line::from(Span::styled(
                line.clone(),
                Style::default().fg(Color::Green),
            )));
        }
    }

    if !state.initial_err_lines.is_empty() {
        if !state.initial_out_lines.is_empty() {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            format!("─── stderr ({} lines):", state.initial_err_lines.len()),
            Style::default().fg(Color::DarkGray),
        )));
        for line in &state.initial_err_lines {
            lines.push(Line::from(Span::styled(
                line.clone(),
                Style::default().fg(Color::LightRed),
            )));
        }
    }

    let total = lines.len();
    let max_scroll = if total > inner_height {
        total - inner_height
    } else {
        0
    };

    let scroll = state.log_scroll.min(max_scroll);
    let start = if total > inner_height {
        max_scroll - scroll
    } else {
        0
    };

    let visible: Vec<Line> = lines.into_iter().skip(start).take(inner_height).collect();

    let p = Paragraph::new(visible).block(block);
    f.render_widget(p, area);
}

fn draw_status_bar(f: &mut ratatui::Frame, state: &DashboardState, area: Rect) {
    let bar = match state.tab {
        Tab::Overview => Line::from(vec![
            Span::styled(" [r]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("estart "),
            Span::styled("[s]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("top "),
            Span::styled("[S]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("tart "),
            Span::styled("[f]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("lush "),
            Span::styled("[Tab]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(" logs "),
            Span::styled("[q]", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw("uit"),
        ]),
        Tab::Logs => Line::from(vec![
            Span::styled(" [1]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(" stdout "),
            Span::styled("[2]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(" stderr "),
            Span::styled("[PgUp/PgDn]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(" scroll "),
            Span::styled("[r]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("estart "),
            Span::styled("[s]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("top "),
            Span::styled("[f]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("lush "),
            Span::styled("[Tab]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(" initial-logs "),
            Span::styled("[q]", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw("uit"),
        ]),
        Tab::InitialLogs => Line::from(vec![
            Span::styled(" [PgUp/PgDn]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(" scroll "),
            Span::styled("[r]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("estart "),
            Span::styled("[s]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("top "),
            Span::styled("[S]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("tart "),
            Span::styled("[f]", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("lush "),
            Span::styled("[Tab]", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(" overview "),
            Span::styled("[q]", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw("uit"),
        ]),
    };

    let p = Paragraph::new(bar).style(Style::default().bg(Color::DarkGray));
    f.render_widget(p, area);
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}...", &s[..max.saturating_sub(3)])
    } else {
        s.to_string()
    }
}
