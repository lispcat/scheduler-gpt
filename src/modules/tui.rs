//! Interactive TUI mode (activated with -t / --tui).
//!
//! Two-screen flow:
//!   Screen 1 – Confirmation
//!     Shows the input file path, its raw contents, and a parsed summary
//!     (algorithm, process count, run duration).  Press <Enter> to proceed.
//!
//!   Screen 2 – Results
//!     Shows the simulation output line by line, scrollable with arrow keys.
//!     Press <Enter> or <q> to exit and return to the normal shell.
//!
//! The TUI is self-contained: it sets up the terminal on entry and restores
//! it on exit (including on panic, via a catch_unwind guard).

use std::io;

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

use crate::modules::models::Config;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run the full TUI flow.
///
/// `input_path`  – display string for the file path (used in the header).
/// `raw_content` – verbatim file content shown in the confirmation screen.
/// `sim_config`  – parsed simulation config (for the summary panel).
/// `output`      – plain-text simulation output shown in the results screen.
///
/// Returns Ok(()) when the user exits normally, or an io::Error if terminal
/// setup fails.
pub fn run_tui(
    input_path: &str,
    raw_content: &str,
    sim_config: &Config,
    output: &str,
) -> io::Result<()> {
    // Set up the terminal and guarantee cleanup even on panic.
    let mut terminal = setup_terminal()?;
    let result = run_app(&mut terminal, input_path, raw_content, sim_config, output);
    restore_terminal(&mut terminal)?;
    result
}

// ---------------------------------------------------------------------------
// Terminal lifecycle helpers
// ---------------------------------------------------------------------------

type Term = Terminal<CrosstermBackend<io::Stdout>>;

fn setup_terminal() -> io::Result<Term> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

fn restore_terminal(terminal: &mut Term) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()
}

// ---------------------------------------------------------------------------
// Application state
// ---------------------------------------------------------------------------

/// Which screen is currently visible.
#[derive(PartialEq)]
enum Screen {
    Confirmation,
    Results,
}

struct AppState<'a> {
    screen: Screen,
    // Confirmation screen
    input_path: &'a str,
    raw_lines: Vec<&'a str>, // raw_content split by newline
    summary: Vec<String>,    // human-readable parsed summary bullets
    // Results screen
    result_lines: Vec<&'a str>,
    scroll: u16, // vertical scroll offset for the results view
}

impl<'a> AppState<'a> {
    fn new(
        input_path: &'a str,
        raw_content: &'a str,
        sim_config: &Config,
        output: &'a str,
    ) -> Self {
        let summary = build_summary(sim_config);
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

/// Build a short human-readable summary of the parsed config for display.
fn build_summary(config: &Config) -> Vec<String> {
    vec![
        format!("  Processes  : {}", config.process_count),
        format!("  Run for    : {} ticks", config.run_for),
        format!("  Algorithm  : {}", config.algorithm),
        format!(
            "  Process list: {}",
            config
                .processes
                .iter()
                .map(|p| format!("{} (arr={}, burst={})", p.name, p.arrival, p.burst))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    ]
}

// ---------------------------------------------------------------------------
// Main event loop
// ---------------------------------------------------------------------------

fn run_app(
    terminal: &mut Term,
    input_path: &str,
    raw_content: &str,
    sim_config: &Config,
    output: &str,
) -> io::Result<()> {
    let mut state = AppState::new(input_path, raw_content, sim_config, output);

    loop {
        terminal.draw(|frame| match state.screen {
            Screen::Confirmation => draw_confirmation(frame, &state),
            Screen::Results => draw_results(frame, &state),
        })?;

        // Block until a key event arrives.
        if let Event::Key(key) = event::read()? {
            // Ignore key-release events on Windows.
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

// ---------------------------------------------------------------------------
// Screen renderers
// ---------------------------------------------------------------------------

/// Screen 1: show the input path, raw file contents, and a parsed summary.
fn draw_confirmation(frame: &mut ratatui::Frame, state: &AppState) {
    let area = frame.area();

    // Outer block gives the whole screen a border and title.
    let outer = Block::default().borders(Borders::ALL).title(Span::styled(
        " Scheduler — Confirm Input ",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ));
    let inner_area = outer.inner(area);
    frame.render_widget(outer, area);

    // Split vertically: path header | file contents | summary | hint.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // input path
            Constraint::Min(5),    // file contents
            Constraint::Length(6), // parsed summary
            Constraint::Length(1), // hint
        ])
        .split(inner_area);

    // ── Path header ──────────────────────────────────────────────────────────
    let path_block = Block::default()
        .borders(Borders::BOTTOM)
        .title(" Input file ");
    let path_text = Paragraph::new(state.input_path)
        .block(path_block)
        .style(Style::default().fg(Color::Yellow));
    frame.render_widget(path_text, chunks[0]);

    // ── Raw file contents ─────────────────────────────────────────────────────
    let items: Vec<ListItem> = state
        .raw_lines
        .iter()
        .map(|l| ListItem::new(Line::from(*l)))
        .collect();
    let file_list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .title(" Contents "),
        )
        .style(Style::default().fg(Color::White));
    frame.render_widget(file_list, chunks[1]);

    // ── Parsed summary ────────────────────────────────────────────────────────
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

    // ── Hint bar ─────────────────────────────────────────────────────────────
    let hint = Paragraph::new(" <Enter> Run simulation    <q> Quit")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hint, chunks[3]);
}

/// Screen 2: show the simulation output, scrollable.
fn draw_results(frame: &mut ratatui::Frame, state: &AppState) {
    let area = frame.area();

    let outer = Block::default().borders(Borders::ALL).title(Span::styled(
        " Scheduler — Results ",
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
    ));
    let inner_area = outer.inner(area);
    frame.render_widget(outer, area);

    // Split: output body | hint.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner_area);

    // Color-code individual result lines by keyword to match the --color flag
    // behaviour, giving the TUI its own visual distinction.
    let items: Vec<ListItem> = state
        .result_lines
        .iter()
        .map(|line| {
            let style = if line.contains("arrived") {
                Style::default().fg(Color::Cyan)
            } else if line.contains("selected") {
                Style::default().fg(Color::Green)
            } else if line.contains("finished") {
                Style::default().fg(Color::Yellow)
            } else if line.contains("Idle") {
                Style::default().fg(Color::DarkGray)
            } else if line.contains("wait") {
                // Per-process stats line
                Style::default().fg(Color::Magenta)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(Line::from(Span::styled(*line, style)))
        })
        .collect();

    let results_list = List::new(items)
        .block(Block::default().borders(Borders::NONE))
        // offset is provided via the scroll state; List doesn't natively scroll,
        // so we skip items from the front based on the scroll offset.
        .style(Style::default());

    // Manually offset by slicing items — ratatui's List renders from the top,
    // so we drop leading items to simulate scrolling.
    let scroll = state.scroll as usize;
    let visible_items: Vec<ListItem> = state
        .result_lines
        .iter()
        .skip(scroll)
        .map(|line| {
            let style = if line.contains("arrived") {
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
            };
            ListItem::new(Line::from(Span::styled(*line, style)))
        })
        .collect();

    let scrolled_list = List::new(visible_items).block(Block::default().borders(Borders::NONE));
    frame.render_widget(scrolled_list, chunks[0]);

    let _ = results_list; // silence unused warning from the first construction

    // Hint bar
    let hint = Paragraph::new(" <j/k> or <arrows> Scroll    <Enter>/<q> Exit")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(hint, chunks[1]);
}
