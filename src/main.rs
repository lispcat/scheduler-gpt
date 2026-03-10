use std::collections::VecDeque;
use std::env;
use std::fmt;
use std::fs;
use std::path::Path;

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Print error message and exit
macro_rules! exit_with_mesg {
    ($($arg:tt)*) => {{
        eprintln!($($arg)*);
        std::process::exit(1);
    }};
}

/// If assertion fails, print error msg and exit with error code 1.
macro_rules! require {
    ($cond:expr, $($arg:tt)*) => {{
        if !$cond {
            eprintln!($($arg)*);
            std::process::exit(1);
        }
    }};
}

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct Config {
    process_count: usize,
    run_for: u32,
    algorithm: Algorithm,
    processes: Vec<Process>,
}

#[derive(Debug, Clone)]
struct Process {
    name: String,
    arrival: u32,
    burst: u32,
    // Mutable simulation fields
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

#[derive(Debug, Clone, PartialEq)]
enum Algorithm {
    Fcfs,
    Sjf,
    Rr(u32), // quantum
}

impl fmt::Display for Algorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Algorithm::Fcfs => write!(f, "First-Come First-Served"),
            Algorithm::Sjf => write!(f, "preemptive Shortest Job First"),
            Algorithm::Rr(q) => write!(f, "Round-Robin"), // TODO: what's this "q" variable doing here?
        }
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    // collect args
    let args: Vec<String> = env::args().collect();

    // assert num args == 1 (first arg in `args` is the path to the exec)
    require!(args.len() != 2, "Usage: scheduler-get <input file>");

    // read file at input_path
    let input_path = &args[1];
    let content = match fs::read_to_string(input_path) {
        Ok(c) => c,
        Err(e) => {
            exit_with_mesg!("Error reading '{}': {}", input_path, e)
        }
    };

    // parse input, save as `config`
    // the function `parse_input` is defined below
    let mut config = match parse_input(&content) {
        Ok(c) => c,
        Err(e) => {
            exit_with_mesg!("{}", e);
        }
    };

    // run simulation.
    // the function `simulate` is defined below
    let events = simulate(&mut config);

    // build output
    // the function `build_output` is defined below
    let output = build_output(&config, &events);

    // derive outfile path
    let out_path = Path::new(input_path)
        .with_extension("out")
        .to_string_lossy()
        .to_string();

    // write output to outpath file
    if let Err(e) = fs::write(&out_path, &output) {
        eprintln!("Error writing '{}': {}", out_path, e);
        std::process::exit(1);
    }

    println!("Output written to {}", out_path);
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse input file.
fn parse_input(content: &str) -> Result<Config, String> {
    let mut process_count: Option<usize> = None;
    let mut run_for: Option<u32> = None;
    let mut algorithm: Option<Algorithm> = None; // TODO: variable `algorithm` is never used it seems.
    let mut quantum: Option<u32> = None;
    let mut use_algo_str: Option<String> = None;
    let mut processes: Vec<Process> = Vec::new();

    // parse infile line by line
    for raw_line in content.lines() {
        // Strip comments
        let line = match raw_line.find('#') {
            Some(idx) => &raw_line[..idx],
            None => raw_line,
        }
        .trim();

        // empty line, skip
        if line.is_empty() {
            continue;
        }

        // tokenize
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }

        // match directive
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
                // process name <N> arrival <A> burst <B>
                let proc = parse_process(&tokens)?;
                processes.push(proc);
            }
            "end" => break,
            other => {
                // Unknown directive – silently ignore or could warn
                let _ = other;
            }
        }
    }

    // Resolve algorithm
    let algorithm = match use_algo_str.as_deref() {
        Some("fcfs") => Algorithm::Fcfs,
        Some("sjf") => Algorithm::Sjf,
        Some("rr") => {
            let q = quantum
                .ok_or_else(|| "Error: missing quantum parameter when use is 'rr'.".to_string())?;
            Algorithm::Rr(q)
        }
        Some(other) => {
            return Err(format!("Error: Unknown algorithm '{}'.", other));
        }
        None => {
            return Err("Error: Missing parameter use.".to_string());
        }
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
    // tokens[0] == "process"
    // Expected: name <N> arrival <A> burst <B>  (in any order after "process")
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

// ---------------------------------------------------------------------------
// Simulation
// ---------------------------------------------------------------------------

fn simulate(config: &mut Config) -> Vec<String> {
    match &config.algorithm.clone() {
        Algorithm::Fcfs => simulate_fcfs(config),
        Algorithm::Sjf => simulate_sjf(config),
        Algorithm::Rr(q) => simulate_rr(config, *q),
    }
}

/// FCFS – non-preemptive, ordered by arrival then name
fn simulate_fcfs(config: &mut Config) -> Vec<String> {
    let run_for = config.run_for;
    let procs = &mut config.processes;
    // Sort by arrival, then name for tie-breaking
    procs.sort_by(|a, b| a.arrival.cmp(&b.arrival).then(a.name.cmp(&b.name)));

    let mut events: Vec<String> = Vec::new();
    let mut current: Option<usize> = None;
    let mut ready: VecDeque<usize> = VecDeque::new();

    for t in 0..run_for {
        // Arrivals
        for (i, p) in procs.iter().enumerate() {
            if p.arrival == t {
                events.push(format!("Time {:3} : {} arrived", t, p.name));
                ready.push_back(i);
            }
        }

        // If CPU is free, pick next
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
                let p = &mut procs[idx];
                p.remaining -= 1;
                // Accumulate wait for all ready processes
                for &ri in &ready {
                    procs[ri].wait += 1;
                }
                if p.remaining == 0 {
                    let finish_t = t + 1;
                    let p = &mut procs[idx];
                    p.finished = true;
                    p.turnaround = finish_t - p.arrival;
                    events.push(format!("Time {:3} : {} finished", finish_t, p.name));
                    current = None;
                }
            }
        }
    }

    events
}

/// Preemptive SJF – at each tick pick the ready process with shortest remaining time
fn simulate_sjf(config: &mut Config) -> Vec<String> {
    let run_for = config.run_for;
    let procs = &mut config.processes;

    let mut events: Vec<String> = Vec::new();
    let mut ready: Vec<usize> = Vec::new(); // indices of arrived, unfinished processes
    let mut current: Option<usize> = None;

    for t in 0..run_for {
        // Arrivals
        for (i, p) in procs.iter().enumerate() {
            if p.arrival == t {
                events.push(format!("Time {:3} : {} arrived", t, p.name));
                ready.push(i);
            }
        }

        // Choose best candidate: shortest remaining, tie-break by name
        let best = ready.iter().copied().min_by(|&a, &b| {
            procs[a]
                .remaining
                .cmp(&procs[b].remaining)
                .then(procs[a].name.cmp(&procs[b].name))
        });

        // Preemption or selection
        match (current, best) {
            (None, Some(b)) => {
                let p = &mut procs[b];
                if p.response.is_none() {
                    p.response = Some(t - p.arrival);
                }
                events.push(format!(
                    "Time {:3} : {} selected (burst {:3})",
                    t, p.name, p.remaining
                ));
                current = Some(b);
            }
            (Some(c), Some(b)) if b != c => {
                // Preempt only if shorter
                if procs[b].remaining < procs[c].remaining {
                    let p = &mut procs[b];
                    if p.response.is_none() {
                        p.response = Some(t - p.arrival);
                    }
                    events.push(format!(
                        "Time {:3} : {} selected (burst {:3})",
                        t, p.name, p.remaining
                    ));
                    current = Some(b);
                }
            }
            _ => {}
        }

        match current {
            None => {
                events.push(format!("Time {:3} : Idle", t));
            }
            Some(idx) => {
                // Accumulate wait for ready processes not running
                let ready_snapshot: Vec<usize> =
                    ready.iter().copied().filter(|&i| i != idx).collect();
                for ri in ready_snapshot {
                    procs[ri].wait += 1;
                }
                procs[idx].remaining -= 1;
                if procs[idx].remaining == 0 {
                    let finish_t = t + 1;
                    procs[idx].finished = true;
                    procs[idx].turnaround = finish_t - procs[idx].arrival;
                    events.push(format!(
                        "Time {:3} : {} finished",
                        finish_t, procs[idx].name
                    ));
                    ready.retain(|&i| i != idx);
                    current = None;
                }
            }
        }
    }

    events
}

/// Round Robin
fn simulate_rr(config: &mut Config, quantum: u32) -> Vec<String> {
    let run_for = config.run_for;
    let procs = &mut config.processes;
    // Sort by arrival for stable ordering
    procs.sort_by(|a, b| a.arrival.cmp(&b.arrival).then(a.name.cmp(&b.name)));

    let mut events: Vec<String> = Vec::new();
    let mut ready: VecDeque<usize> = VecDeque::new();
    let mut current: Option<usize> = None;
    let mut quantum_left: u32 = 0;

    for t in 0..run_for {
        // Arrivals – insert in order, but AFTER current if same tick as selection
        for (i, p) in procs.iter().enumerate() {
            if p.arrival == t {
                events.push(format!("Time {:3} : {} arrived", t, p.name));
                // Don't add if already in ready or running
                if current != Some(i) && !ready.contains(&i) {
                    ready.push_back(i);
                }
            }
        }

        // Quantum expired or process finished: re-queue current, pick next
        if let Some(idx) = current {
            if quantum_left == 0 {
                if !procs[idx].finished {
                    // Check for new arrivals that snuck in this tick
                    ready.push_back(idx);
                }
                current = None;
            }
        }

        // Select next if idle
        if current.is_none() {
            if let Some(idx) = ready.pop_front() {
                let p = &mut procs[idx];
                if p.response.is_none() {
                    p.response = Some(t - p.arrival);
                }
                events.push(format!(
                    "Time {:3} : {} selected (burst {:3})",
                    t, p.name, p.remaining
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
                // Accumulate wait for all processes in ready queue
                for &ri in &ready {
                    procs[ri].wait += 1;
                }
                procs[idx].remaining -= 1;
                quantum_left -= 1;
                if procs[idx].remaining == 0 {
                    let finish_t = t + 1;
                    procs[idx].finished = true;
                    procs[idx].turnaround = finish_t - procs[idx].arrival;
                    events.push(format!(
                        "Time {:3} : {} finished",
                        finish_t, procs[idx].name
                    ));
                    current = None;
                    quantum_left = 0;
                }
            }
        }
    }

    events
}

// ---------------------------------------------------------------------------
// Output formatting
// ---------------------------------------------------------------------------

fn build_output(config: &Config, events: &[String]) -> String {
    let mut lines: Vec<String> = Vec::new();

    lines.push(format!("{} processes", config.process_count));

    match &config.algorithm {
        Algorithm::Fcfs => lines.push("Using First-Come First-Served".to_string()),
        Algorithm::Sjf => lines.push("Using preemptive Shortest Job First".to_string()),
        Algorithm::Rr(q) => {
            lines.push("Using Round-Robin".to_string());
            lines.push(format!("Quantum {}", q));
        }
    }

    for e in events {
        lines.push(e.clone());
    }

    lines.push(format!("Finished at time {:3}", config.run_for));
    lines.push(String::new());

    // Per-process summary, sorted by name
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

    lines.join("\n")
}
