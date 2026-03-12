use std::collections::VecDeque;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Terminal,
};

// =============================================================================
// Args
// =============================================================================

/// Process scheduling algorithm simulator.
///
/// Reads an input file describing processes and a scheduling algorithm,
/// runs the simulation, and writes the results to <input>.out in the
/// current working directory.
#[derive(Parser, Debug)]
#[command(
    name = "scheduler-get",
    version,
    about,
    override_usage = "scheduler-get [OPTIONS] <input file>"
)]
struct Args {
    /// Path to the .in input file describing the workload.
    input_file: PathBuf,

    /// Colorize output with ANSI escape codes (non-TUI mode only).
    ///
    /// Events are color-coded by type:
    ///   arrived  -> cyan
    ///   selected -> green
    ///   finished -> yellow
    ///   Idle     -> dark grey
    #[arg(short = 'c', long = "color", default_value_t = false)]
    color: bool,

    /// Print output to stdout (non-TUI mode only).
    ///
    /// Does not affect whether the .out file is written; combine with -d to
    /// suppress the file entirely.
    #[arg(short = 'p', long = "print", default_value_t = false)]
    print: bool,

    /// Disable writing the .out file.
    ///
    /// Works in both normal and TUI mode. In TUI mode the simulation still
    /// runs; results are shown on screen but nothing is written to disk.
    #[arg(short = 'd', long = "no-file", default_value_t = false)]
    no_file: bool,

    /// Open an interactive TUI to preview input and view results.
    ///
    /// Two screens:
    ///   1. Confirmation: shows the input path, file contents, and parsed
    ///      summary.  Press <Enter> to run the simulation.
    ///   2. Results: shows the simulation output.
    ///      Press <Enter> or <q> to exit.
    #[arg(short = 't', long = "tui", default_value_t = false)]
    tui: bool,
}

// =============================================================================
// Models
// =============================================================================

/// A single process, including both its static definition and mutable
/// simulation state that gets updated as the scheduler runs.
#[derive(Debug, Clone)]
struct Process {
    // --- Static fields (set at parse time, never change) ---
    name: String,
    arrival: u32,
    burst: u32,

    // --- Mutable simulation fields (updated during simulation) ---
    remaining: u32,
    wait: u32,
    turnaround: u32,
    response: Option<u32>,
    finished: bool,
    started: bool,
}

impl Process {
    fn new(name: String, arrival: u32, burst: u32) -> Self {
        Process {
            name,
            arrival,
            burst,
            remaining: burst,
            wait: 0,
            turnaround: 0,
            response: None,
            finished: false,
            started: false,
        }
    }
}

/// The three supported scheduling algorithms.
#[derive(Debug, Clone, PartialEq)]
enum Algorithm {
    Fcfs,
    Sjf,
    Rr(u32),
}

impl fmt::Display for Algorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Algorithm::Fcfs => write!(f, "First-Come First-Served"),
            Algorithm::Sjf => write!(f, "preemptive Shortest Job First"),
            Algorithm::Rr(_) => write!(f, "Round-Robin"),
        }
    }
}

/// All configuration parsed from the input file.
#[derive(Debug)]
struct Config {
    process_count: usize,
    run_for: u32,
    algorithm: Algorithm,
    processes: Vec<Process>,
}

// =============================================================================
// Parser
// =============================================================================

fn parse_input(content: &str) -> Result<Config, String> {
    let mut process_count: Option<usize> = None;
    let mut run_for: Option<u32> = None;
    let mut quantum: Option<u32> = None;
    let mut use_algo_str: Option<String> = None;
    let mut processes: Vec<Process> = Vec::new();

    for raw_line in content.lines() {
        let line = match raw_line.find('#') {
            Some(idx) => &raw_line[..idx],
            None => raw_line,
        }
        .trim();

        if line.is_empty() {
            continue;
        }

        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }

        match tokens[0] {
            "processcount" => {
                let n = tokens
                    .get(1)
                    .ok_or("Error: Missing parameter processcount.")?
                    .parse::<usize>()
                    .map_err(|_| "Error: processcount must be an integer.".to_string())?;
                process_count = Some(n);
            }
            "runfor" => {
                let n = tokens
                    .get(1)
                    .ok_or("Error: Missing parameter runfor.")?
                    .parse::<u32>()
                    .map_err(|_| "Error: runfor must be an integer.".to_string())?;
                run_for = Some(n);
            }
            "use" => {
                let algo = tokens.get(1).ok_or("Error: Missing parameter use.")?;
                use_algo_str = Some(algo.to_string());
            }
            "quantum" => {
                let q = tokens
                    .get(1)
                    .ok_or("Error: Missing parameter quantum.")?
                    .parse::<u32>()
                    .map_err(|_| "Error: quantum must be an integer.".to_string())?;
                quantum = Some(q);
            }
            "process" => {
                let proc = parse_process(&tokens)?;
                processes.push(proc);
            }
            "end" => break,
            _ => {}
        }
    }

    let algorithm = match use_algo_str.as_deref() {
        Some("fcfs") => Algorithm::Fcfs,
        Some("sjf") => Algorithm::Sjf,
        Some("rr") => {
            let q = quantum
                .ok_or_else(|| "Error: missing quantum parameter when use is 'rr'.".to_string())?;
            Algorithm::Rr(q)
        }
        Some(other) => return Err(format!("Error: Unknown algorithm '{}'.", other)),
        None => return Err("Error: Missing parameter use.".to_string()),
    };

    let process_count =
        process_count.ok_or_else(|| "Error: Missing parameter processcount.".to_string())?;
    let run_for = run_for.ok_or_else(|| "Error: Missing parameter runfor.".to_string())?;

    Ok(Config {
        process_count,
        run_for,
        algorithm,
        processes,
    })
}

fn parse_process(tokens: &[&str]) -> Result<Process, String> {
    let mut name: Option<String> = None;
    let mut arrival: Option<u32> = None;
    let mut burst: Option<u32> = None;

    let mut i = 1;
    while i < tokens.len() {
        match tokens[i] {
            "name" => {
                name = Some(
                    tokens
                        .get(i + 1)
                        .ok_or("Error: Missing parameter name.")?
                        .to_string(),
                );
                i += 2;
            }
            "arrival" => {
                arrival = Some(
                    tokens
                        .get(i + 1)
                        .ok_or("Error: Missing parameter arrival.")?
                        .parse::<u32>()
                        .map_err(|_| "Error: arrival must be an integer.".to_string())?,
                );
                i += 2;
            }
            "burst" => {
                burst = Some(
                    tokens
                        .get(i + 1)
                        .ok_or("Error: Missing parameter burst.")?
                        .parse::<u32>()
                        .map_err(|_| "Error: burst must be an integer.".to_string())?,
                );
                i += 2;
            }
            _ => {
                i += 1;
            }
        }
    }

    let name = name.ok_or_else(|| "Error: Missing parameter name.".to_string())?;
    let arrival = arrival.ok_or_else(|| "Error: Missing parameter arrival.".to_string())?;
    let burst = burst.ok_or_else(|| "Error: Missing parameter burst.".to_string())?;

    Ok(Process::new(name, arrival, burst))
}

// =============================================================================
// Scheduler
// =============================================================================

fn simulate(config: &mut Config) -> Vec<String> {
    match &config.algorithm.clone() {
        Algorithm::Fcfs => simulate_fcfs(config),
        Algorithm::Sjf => simulate_sjf(config),
        Algorithm::Rr(q) => simulate_rr(config, *q),
    }
}

fn simulate_fcfs(config: &mut Config) -> Vec<String> {
    let run_for = config.run_for;
    let procs = &mut config.processes;

    procs.sort_by(|a, b| a.arrival.cmp(&b.arrival).then(a.name.cmp(&b.name)));

    let mut events: Vec<String> = Vec::new();
    let mut current: Option<usize> = None;
    let mut ready: VecDeque<usize> = VecDeque::new();
    let mut pending_finish: Option<usize> = None;

    for t in 0..run_for {
        for (i, p) in procs.iter().enumerate() {
            if p.arrival == t {
                events.push(format!("Time {:3} : {} arrived", t, p.name));
                ready.push_back(i);
            }
        }

        if let Some(idx) = pending_finish.take() {
            events.push(format!("Time {:3} : {} finished", t, procs[idx].name));
        }

        if current.is_none() {
            if let Some(idx) = ready.pop_front() {
                let p = &mut procs[idx];
                if p.response.is_none() {
                    p.response = Some(t.saturating_sub(p.arrival));
                }
                p.started = true;
                events.push(format!(
                    "Time {:3} : {} selected (burst {:3})",
                    t, p.name, p.remaining
                ));
                current = Some(idx);
            }
        }

        match current {
            None => {
                events.push(format!("Time {:3} : Idle", t));
            }
            Some(idx) => {
                let waiting: Vec<usize> = ready.iter().copied().collect();
                for ri in waiting {
                    procs[ri].wait += 1;
                }
                procs[idx].remaining -= 1;
                if procs[idx].remaining == 0 {
                    procs[idx].finished = true;
                    procs[idx].turnaround = (t + 1) - procs[idx].arrival;
                    pending_finish = Some(idx);
                    current = None;
                }
            }
        }
    }

    if let Some(idx) = pending_finish.take() {
        events.push(format!("Time {:3} : {} finished", run_for, procs[idx].name));
    }

    events
}

fn simulate_sjf(config: &mut Config) -> Vec<String> {
    let run_for = config.run_for;
    let procs = &mut config.processes;

    let mut events: Vec<String> = Vec::new();
    let mut ready: Vec<usize> = Vec::new();
    let mut current: Option<usize> = None;
    let mut pending_finish: Option<usize> = None;

    for t in 0..run_for {
        for (i, p) in procs.iter().enumerate() {
            if p.arrival == t {
                events.push(format!("Time {:3} : {} arrived", t, p.name));
                ready.push(i);
            }
        }

        if let Some(idx) = pending_finish.take() {
            events.push(format!("Time {:3} : {} finished", t, procs[idx].name));
        }

        let best = ready.iter().copied().min_by(|&a, &b| {
            procs[a]
                .remaining
                .cmp(&procs[b].remaining)
                .then(procs[a].name.cmp(&procs[b].name))
        });

        match (current, best) {
            (None, Some(b)) => {
                if procs[b].response.is_none() {
                    procs[b].response = Some(t - procs[b].arrival);
                }
                events.push(format!(
                    "Time {:3} : {} selected (burst {:3})",
                    t, procs[b].name, procs[b].remaining
                ));
                current = Some(b);
            }
            (Some(c), Some(b)) if b != c && procs[b].remaining < procs[c].remaining => {
                if procs[b].response.is_none() {
                    procs[b].response = Some(t - procs[b].arrival);
                }
                events.push(format!(
                    "Time {:3} : {} selected (burst {:3})",
                    t, procs[b].name, procs[b].remaining
                ));
                current = Some(b);
            }
            _ => {}
        }

        match current {
            None => {
                events.push(format!("Time {:3} : Idle", t));
            }
            Some(idx) => {
                let waiting: Vec<usize> = ready.iter().copied().filter(|&i| i != idx).collect();
                for ri in waiting {
                    procs[ri].wait += 1;
                }
                procs[idx].remaining -= 1;
                if procs[idx].remaining == 0 {
                    procs[idx].finished = true;
                    procs[idx].turnaround = (t + 1) - procs[idx].arrival;
                    ready.retain(|&i| i != idx);
                    pending_finish = Some(idx);
                    current = None;
                }
            }
        }
    }

    if let Some(idx) = pending_finish.take() {
        events.push(format!("Time {:3} : {} finished", run_for, procs[idx].name));
    }

    events
}

fn simulate_rr(config: &mut Config, quantum: u32) -> Vec<String> {
    let run_for = config.run_for;
    let procs = &mut config.processes;

    procs.sort_by(|a, b| a.arrival.cmp(&b.arrival).then(a.name.cmp(&b.name)));

    let mut events: Vec<String> = Vec::new();
    let mut ready: VecDeque<usize> = VecDeque::new();
    let mut current: Option<usize> = None;
    let mut quantum_left: u32 = 0;
    let mut pending_finish: Option<usize> = None;

    for t in 0..run_for {
        for (i, p) in procs.iter().enumerate() {
            if p.arrival == t {
                events.push(format!("Time {:3} : {} arrived", t, p.name));
                if current != Some(i) && !ready.contains(&i) {
                    ready.push_back(i);
                }
            }
        }

        if let Some(idx) = pending_finish.take() {
            events.push(format!("Time {:3} : {} finished", t, procs[idx].name));
        }

        if let Some(idx) = current {
            if quantum_left == 0 {
                if !procs[idx].finished {
                    ready.push_back(idx);
                }
                current = None;
            }
        }

        if current.is_none() {
            if let Some(idx) = ready.pop_front() {
                if procs[idx].response.is_none() {
                    procs[idx].response = Some(t - procs[idx].arrival);
                }
                events.push(format!(
                    "Time {:3} : {} selected (burst {:3})",
                    t, procs[idx].name, procs[idx].remaining
                ));
                current = Some(idx);
                quantum_left = quantum;
            }
        }

        match current {
            None => {
                events.push(format!("Time {:3} : Idle", t));
            }
            Some(idx) => {
                let waiting: Vec<usize> = ready.iter().copied().collect();
                for ri in waiting {
                    procs[ri].wait += 1;
                }
                procs[idx].remaining -= 1;
                quantum_left -= 1;
                if procs[idx].remaining == 0 {
                    procs[idx].finished = true;
                    procs[idx].turnaround = (t + 1) - procs[idx].arrival;
                    pending_finish = Some(idx);
                    current = None;
                    quantum_left = 0;
                }
            }
        }
    }

    if let Some(idx) = pending_finish.take() {
        events.push(format!("Time {:3} : {} finished", run_for, procs[idx].name));
    }

    events
}

// =============================================================================
// Output
// =============================================================================

fn ansi_cyan(s: &str, color: bool) -> String {
    if color {
        format!("\x1b[36m{}\x1b[0m", s)
    } else {
        s.to_string()
    }
}
fn ansi_green(s: &str, color: bool) -> String {
    if color {
        format!("\x1b[32m{}\x1b[0m", s)
    } else {
        s.to_string()
    }
}
fn ansi_yellow(s: &str, color: bool) -> String {
    if color {
        format!("\x1b[33m{}\x1b[0m", s)
    } else {
        s.to_string()
    }
}
fn ansi_dark_grey(s: &str, color: bool) -> String {
    if color {
        format!("\x1b[90m{}\x1b[0m", s)
    } else {
        s.to_string()
    }
}

fn colorize_event(line: &str, color: bool) -> String {
    if !color {
        return line.to_string();
    }
    if line.contains("arrived") {
        ansi_cyan(line, color)
    } else if line.contains("selected") {
        ansi_green(line, color)
    } else if line.contains("finished") {
        ansi_yellow(line, color)
    } else if line.contains("Idle") {
        ansi_dark_grey(line, color)
    } else {
        line.to_string()
    }
}

fn build_output(config: &Config, events: &[String], color: bool) -> String {
    let mut lines: Vec<String> = Vec::new();

    lines.push(format!("{:3} processes", config.process_count));
    match &config.algorithm {
        Algorithm::Fcfs => lines.push("Using First-Come First-Served".to_string()),
        Algorithm::Sjf => lines.push("Using preemptive Shortest Job First".to_string()),
        Algorithm::Rr(q) => {
            lines.push("Using Round-Robin".to_string());
            lines.push(format!("Quantum {:3}", q));
            lines.push(String::new());
        }
    }

    for e in events {
        lines.push(colorize_event(e, color));
    }

    lines.push(format!("Finished at time {:3}", config.run_for));
    lines.push(String::new());

    let mut sorted_procs = config.processes.clone();
    sorted_procs.sort_by(|a, b| a.name.cmp(&b.name));

    for p in &sorted_procs {
        if p.finished {
            let response = p.response.unwrap_or(0);
            lines.push(format!(
                "{} wait {:3} turnaround {:3} response {:3}",
                p.name, p.wait, p.turnaround, response
            ));
        } else {
            lines.push(format!("{} did not finish", p.name));
        }
    }

    lines.join("\n") + "\n"
}

// =============================================================================
// TUI
// =============================================================================

type Term = Terminal<CrosstermBackend<io::Stdout>>;

fn tui_setup() -> io::Result<Term> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Terminal::new(CrosstermBackend::new(stdout))
}

fn tui_restore(terminal: &mut Term) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()
}

#[derive(PartialEq)]
enum Screen {
    Confirmation,
    Results,
}

struct AppState<'a> {
    screen: Screen,
    input_path: &'a str,
    raw_lines: Vec<&'a str>,
    summary: Vec<String>,
    result_lines: Vec<&'a str>,
    scroll: u16,
}

impl<'a> AppState<'a> {
    fn new(
        input_path: &'a str,
        raw_content: &'a str,
        sim_config: &Config,
        output: &'a str,
    ) -> Self {
        let summary = vec![
            format!("  Processes  : {}", sim_config.process_count),
            format!("  Run for    : {} ticks", sim_config.run_for),
            format!("  Algorithm  : {}", sim_config.algorithm),
            format!(
                "  Process list: {}",
                sim_config
                    .processes
                    .iter()
                    .map(|p| format!("{} (arr={}, burst={})", p.name, p.arrival, p.burst))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ];
        AppState {
            screen: Screen::Confirmation,
            input_path,
            raw_lines: raw_content.lines().collect(),
            summary,
            result_lines: output.lines().collect(),
            scroll: 0,
        }
    }
}

fn run_tui(
    input_path: &str,
    raw_content: &str,
    sim_config: &Config,
    output: &str,
) -> io::Result<()> {
    let mut terminal = tui_setup()?;
    let result = tui_event_loop(&mut terminal, input_path, raw_content, sim_config, output);
    tui_restore(&mut terminal)?;
    result
}

fn tui_event_loop(
    terminal: &mut Term,
    input_path: &str,
    raw_content: &str,
    sim_config: &Config,
    output: &str,
) -> io::Result<()> {
    let mut state = AppState::new(input_path, raw_content, sim_config, output);

    loop {
        terminal.draw(|frame| match state.screen {
            Screen::Confirmation => tui_draw_confirmation(frame, &state),
            Screen::Results => tui_draw_results(frame, &state),
        })?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match state.screen {
                Screen::Confirmation => match key.code {
                    KeyCode::Enter => state.screen = Screen::Results,
                    KeyCode::Char('q') => return Ok(()),
                    _ => {}
                },
                Screen::Results => match key.code {
                    KeyCode::Enter | KeyCode::Char('q') => return Ok(()),
                    KeyCode::Down | KeyCode::Char('j') => {
                        state.scroll = state.scroll.saturating_add(1)
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        state.scroll = state.scroll.saturating_sub(1)
                    }
                    _ => {}
                },
            }
        }
    }
}

fn tui_draw_confirmation(frame: &mut ratatui::Frame, state: &AppState) {
    let area = frame.area();

    let outer = Block::default().borders(Borders::ALL).title(Span::styled(
        " Scheduler — Confirm Input ",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ));
    let inner_area = outer.inner(area);
    frame.render_widget(outer, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(6),
            Constraint::Length(1),
        ])
        .split(inner_area);

    let path_text = Paragraph::new(state.input_path)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .title(" Input file "),
        )
        .style(Style::default().fg(Color::Yellow));
    frame.render_widget(path_text, chunks[0]);

    let file_items: Vec<ListItem> = state
        .raw_lines
        .iter()
        .map(|l| ListItem::new(Line::from(*l)))
        .collect();
    let file_list = List::new(file_items)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .title(" Contents "),
        )
        .style(Style::default().fg(Color::White));
    frame.render_widget(file_list, chunks[1]);

    let summary_text = Text::from(
        state
            .summary
            .iter()
            .map(|s| Line::from(s.as_str()))
            .collect::<Vec<_>>(),
    );
    let summary = Paragraph::new(summary_text)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .title(" Parsed summary "),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(summary, chunks[2]);

    let hint = Paragraph::new(" <Enter> Run simulation    <q> Quit")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hint, chunks[3]);
}

fn tui_line_style(line: &str) -> Style {
    if line.contains("arrived") {
        Style::default().fg(Color::Cyan)
    } else if line.contains("selected") {
        Style::default().fg(Color::Green)
    } else if line.contains("finished") {
        Style::default().fg(Color::Yellow)
    } else if line.contains("Idle") {
        Style::default().fg(Color::DarkGray)
    } else if line.contains("wait") {
        Style::default().fg(Color::Magenta)
    } else {
        Style::default().fg(Color::White)
    }
}

fn tui_draw_results(frame: &mut ratatui::Frame, state: &AppState) {
    let area = frame.area();

    let outer = Block::default().borders(Borders::ALL).title(Span::styled(
        " Scheduler — Results ",
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
    ));
    let inner_area = outer.inner(area);
    frame.render_widget(outer, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner_area);

    let visible_items: Vec<ListItem> = state
        .result_lines
        .iter()
        .skip(state.scroll as usize)
        .map(|line| ListItem::new(Line::from(Span::styled(*line, tui_line_style(line)))))
        .collect();

    let results_list = List::new(visible_items).block(Block::default().borders(Borders::NONE));
    frame.render_widget(results_list, chunks[0]);

    let hint = Paragraph::new(" <j/k> or <arrows> Scroll    <Enter>/<q> Exit")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hint, chunks[1]);
}

// =============================================================================
// main
// =============================================================================

fn write_output_file(input_path: &Path, content: &str) {
    let out_filename = input_path
        .file_stem()
        .expect("input path has no filename")
        .to_string_lossy()
        .to_string()
        + ".out";
    let out_path = Path::new(&out_filename).to_path_buf();

    if let Err(e) = fs::write(&out_path, content) {
        eprintln!("Error writing '{}': {}", out_path.display(), e);
        std::process::exit(1);
    }

    eprintln!("Output written to {}", out_path.display());
}

fn main() {
    let args = Args::parse();
    let input_path = &args.input_file;

    let content = match fs::read_to_string(input_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading '{}': {}", input_path.display(), e);
            std::process::exit(1);
        }
    };

    let mut sim_config = match parse_input(&content) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    let events = simulate(&mut sim_config);
    let plain = build_output(&sim_config, &events, false);

    if !args.no_file {
        write_output_file(input_path, &plain);
    }

    if args.tui {
        let input_path_str = input_path.to_string_lossy();
        if let Err(e) = run_tui(&input_path_str, &content, &sim_config, &plain) {
            eprintln!("TUI error: {}", e);
            std::process::exit(1);
        }
    } else {
        if args.print {
            let display = build_output(&sim_config, &events, args.color);
            print!("{}", display);
        }
    }
}
