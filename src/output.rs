use crate::models::{Algorithm, Config};

/// Build the complete output string from the simulation results.
///
/// Format:
///   <N> processes
///   Using <algorithm>
///   [Quantum <Q>]            <- only for RR, followed by a blank line
///   Time  X : <event> ...    <- one per tick with an event
///   Finished at time  Y
///                            <- blank separator line
///   <name> wait W turnaround T response R   <- per process, sorted by name
///   <name> did not finish                   <- for processes that didn't complete
pub fn build_output(config: &Config, events: &[String]) -> String {
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

    // Event log lines produced by the simulation.
    for e in events {
        lines.push(e.clone());
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
