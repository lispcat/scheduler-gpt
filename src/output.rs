use crate::models::{Algorithm, Config};

// ---------------------------------------------------------------------------
// ANSI color helpers
// ---------------------------------------------------------------------------

// Each helper wraps a string in the appropriate ANSI escape sequence when
// `color` is true, and returns it unchanged when false.  Using explicit reset
// codes rather than a crate keeps us dependency-free in this module.

fn cyan(s: &str, color: bool) -> String {
    if color {
        format!("\x1b[36m{}\x1b[0m", s)
    } else {
        s.to_string()
    }
}

fn green(s: &str, color: bool) -> String {
    if color {
        format!("\x1b[32m{}\x1b[0m", s)
    } else {
        s.to_string()
    }
}

fn yellow(s: &str, color: bool) -> String {
    if color {
        format!("\x1b[33m{}\x1b[0m", s)
    } else {
        s.to_string()
    }
}

fn dark_grey(s: &str, color: bool) -> String {
    if color {
        format!("\x1b[90m{}\x1b[0m", s)
    } else {
        s.to_string()
    }
}

/// Colorize a single event line based on its keyword.
/// Lines are matched by their content so the function stays robust to
/// varying timestamp widths.
fn colorize_event(line: &str, color: bool) -> String {
    if !color {
        return line.to_string();
    }
    if line.contains("arrived") {
        cyan(line, color)
    } else if line.contains("selected") {
        green(line, color)
    } else if line.contains("finished") {
        yellow(line, color)
    } else if line.contains("Idle") {
        dark_grey(line, color)
    } else {
        line.to_string()
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Build the complete output string from the simulation results.
///
/// When `color` is true, event lines are wrapped in ANSI escape codes:
///   arrived  -> cyan
///   selected -> green
///   finished -> yellow
///   Idle     -> dark grey
///
/// Format:
///   <N> processes
///   Using <algorithm>
///   [Quantum <Q>]            <- only for RR, followed by a blank line
///   Time  X : <event> ...    <- one per tick with an event
///   Finished at time  Y
///                            <- blank separator line
///   <n> wait W turnaround T response R   <- per process, sorted by name
///   <n> did not finish                   <- for processes that didn't complete
pub fn build_output(config: &Config, events: &[String], color: bool) -> String {
    let mut lines: Vec<String> = Vec::new();

    // Header: process count (3-wide) and algorithm name.
    lines.push(format!("{:3} processes", config.process_count));
    match &config.algorithm {
        Algorithm::Fcfs => lines.push("Using First-Come First-Served".to_string()),
        Algorithm::Sjf => lines.push("Using preemptive Shortest Job First".to_string()),
        Algorithm::Rr(q) => {
            lines.push("Using Round-Robin".to_string());
            // Quantum value is 3-wide right-justified, followed by a blank line.
            lines.push(format!("Quantum {:3}", q));
            lines.push(String::new());
        }
    }

    // Event log: colorize each line if requested.
    for e in events {
        lines.push(colorize_event(e, color));
    }

    // Footer: final time tick.
    lines.push(format!("Finished at time {:3}", config.run_for));
    lines.push(String::new()); // blank separator line before per-process stats

    // Per-process summary, sorted alphabetically by name for deterministic output.
    let mut sorted_procs = config.processes.clone();
    sorted_procs.sort_by(|a, b| a.name.cmp(&b.name));

    for p in &sorted_procs {
        if p.finished {
            let response = p.response.unwrap_or(0);
            // All three stats are 3-wide right-justified, single space between fields.
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
