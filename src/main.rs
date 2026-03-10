use std::collections::VecDeque;
use std::env;
use std::fmt;
use std::fs;
use std::path::Path;

// ---------------------------------------------------------------------------
// Helper macros
// ---------------------------------------------------------------------------

/// Immediately print an error message to stderr and exit with code 1.
/// Usage: exit_with_mesg!("something went wrong: {}", detail)
macro_rules! exit_with_mesg {
    ($($arg:tt)*) => {{
        eprintln!($($arg)*);
        std::process::exit(1);
    }};
}

/// Assert a condition is true; if not, print a message to stderr and exit.
/// This is used for upfront validation (e.g. checking CLI arg count).
/// Usage: require!(some_bool, "Error: {}", reason)
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

/// All configuration parsed from the input file.
#[derive(Debug)]
struct Config {
    process_count: usize, // declared count of processes (from `processcount` directive)
    run_for: u32,         // total simulation duration in time ticks
    algorithm: Algorithm, // which scheduling algorithm to use
    processes: Vec<Process>, // list of all processes
}

/// A single process, including both its static definition and mutable
/// simulation state that gets updated as the scheduler runs.
#[derive(Debug, Clone)]
struct Process {
    // --- Static fields (set at parse time, never change) ---
    name: String, // process identifier, e.g. "A"
    arrival: u32, // time tick at which this process enters the system
    burst: u32,   // total CPU time needed to complete

    // --- Mutable simulation fields (updated during simulation) ---
    remaining: u32,        // CPU time still needed (counts down from `burst`)
    wait: u32,             // total time spent in ready queue (not running)
    turnaround: u32,       // total time from arrival to completion (finish - arrival)
    response: Option<u32>, // time from arrival until first CPU assignment; None until set
    finished: bool,        // true once remaining reaches 0
    started: bool,         // true once the process has been selected at least once
}

impl Process {
    /// Construct a new process with fully initialised simulation state.
    fn new(name: String, arrival: u32, burst: u32) -> Self {
        Process {
            name,
            arrival,
            burst,
            remaining: burst, // starts equal to burst; counts down during simulation
            wait: 0,
            turnaround: 0,
            response: None, // will be set the first time this process is selected
            finished: false,
            started: false,
        }
    }
}

/// The three supported scheduling algorithms.
/// RR carries its quantum length as an associated value so it is always
/// available wherever an Algorithm value is used.
#[derive(Debug, Clone, PartialEq)]
enum Algorithm {
    Fcfs,    // First-Come First-Served (non-preemptive)
    Sjf,     // Shortest Job First (preemptive / SRTF variant)
    Rr(u32), // Round-Robin with the given quantum length
}

/// Human-readable algorithm names used in the output header.
impl fmt::Display for Algorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Algorithm::Fcfs => write!(f, "First-Come First-Served"),
            Algorithm::Sjf => write!(f, "preemptive Shortest Job First"),
            // The quantum value is stored in the enum but not printed here;
            // build_output prints it on a separate "Quantum N" line.
            Algorithm::Rr(_q) => write!(f, "Round-Robin"),
        }
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    // Collect all command-line arguments into a Vec.
    // args[0] is always the path to the executable itself, so we expect
    // exactly 2 elements total: [executable, input_file].
    let args: Vec<String> = env::args().collect();

    // Exactly one user-supplied argument is required (the input filename).
    require!(args.len() == 2, "Usage: scheduler-get <input file>");

    // Read the entire input file into a String.
    let input_path = &args[1];
    let content = match fs::read_to_string(input_path) {
        Ok(c) => c,
        Err(e) => exit_with_mesg!("Error reading '{}': {}", input_path, e),
    };

    // Parse the input text into a Config struct.
    // Any structural errors (missing directives, bad values) cause an early exit.
    let mut config = match parse_input(&content) {
        Ok(c) => c,
        Err(e) => exit_with_mesg!("{}", e),
    };

    // Run the chosen scheduling algorithm and collect the event log.
    let events = simulate(&mut config);

    // Format the full output text from the event log and process stats.
    let output = build_output(&config, &events);

    // Derive the output filename: take only the file stem (no directory
    // component) and add ".out", then write into the current working directory.
    // e.g. running with "../tests/data/input.in" produces "./input.out"
    let out_filename = Path::new(input_path)
        .file_stem() // "input"  (drops directory + extension)
        .expect("input path has no filename")
        .to_string_lossy()
        .to_string()
        + ".out"; // -> "input.out"
    let out_path = Path::new(&out_filename).to_path_buf();

    // Write output file; exit with an error if the write fails.
    if let Err(e) = fs::write(&out_path, &output) {
        eprintln!("Error writing '{}': {}", out_path.display(), e);
        std::process::exit(1);
    }

    println!("Output written to {}", out_path.display());
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse the full input file content into a Config.
///
/// Processes the file line by line:
///   - Strips everything after '#' (comments)
///   - Splits each line into whitespace-delimited tokens
///   - Dispatches on the first token (the directive keyword)
///   - Stops at the "end" directive
///
/// Returns Err with a descriptive message if any required directive is absent
/// or has an invalid value.
fn parse_input(content: &str) -> Result<Config, String> {
    // Accumulate optional fields; we validate presence after the loop.
    let mut process_count: Option<usize> = None;
    let mut run_for: Option<u32> = None;
    let mut quantum: Option<u32> = None;
    let mut use_algo_str: Option<String> = None;
    let mut processes: Vec<Process> = Vec::new();

    for raw_line in content.lines() {
        // Remove inline comments: everything from '#' to end of line.
        let line = match raw_line.find('#') {
            Some(idx) => &raw_line[..idx],
            None => raw_line,
        }
        .trim();

        if line.is_empty() {
            continue;
        }

        // Split the (comment-stripped) line into tokens.
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }

        // Dispatch on the directive keyword (first token).
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
                // Store the raw algorithm string; we resolve it after the loop
                // so that "quantum" (which may appear after "use") is available.
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
                // Delegate per-process field parsing to a dedicated helper.
                let proc = parse_process(&tokens)?;
                processes.push(proc);
            }
            "end" => break, // explicit end-of-file marker; stop parsing
            _ => {
                // Unknown directive — silently ignore to allow future extensions.
            }
        }
    }

    // Resolve the algorithm now that both "use" and (optional) "quantum" are parsed.
    let algorithm = match use_algo_str.as_deref() {
        Some("fcfs") => Algorithm::Fcfs,
        Some("sjf") => Algorithm::Sjf,
        Some("rr") => {
            // RR requires a quantum; fail with a specific message if absent.
            let q = quantum
                .ok_or_else(|| "Error: missing quantum parameter when use is 'rr'.".to_string())?;
            Algorithm::Rr(q)
        }
        Some(other) => return Err(format!("Error: Unknown algorithm '{}'.", other)),
        None => return Err("Error: Missing parameter use.".to_string()),
    };

    // Fail fast if any top-level required directives were missing.
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

/// Parse a single "process" line into a Process struct.
///
/// `tokens` is the already-split line, with tokens[0] == "process".
/// The remaining tokens are key-value pairs that can appear in any order:
///   name <string>   arrival <u32>   burst <u32>
///
/// The loop advances by 2 when it recognises a keyword (consuming both the
/// keyword and its value), and by 1 for anything unrecognised (skipping it).
/// This makes the parser order-independent and tolerant of extra tokens.
///
/// Returns Err if any of the three required fields is absent.
fn parse_process(tokens: &[&str]) -> Result<Process, String> {
    let mut name: Option<String> = None;
    let mut arrival: Option<u32> = None;
    let mut burst: Option<u32> = None;

    let mut i = 1; // start after "process"
    while i < tokens.len() {
        match tokens[i] {
            "name" => {
                name = Some(
                    tokens
                        .get(i + 1)
                        .ok_or("Error: Missing parameter name.")?
                        .to_string(),
                );
                i += 2; // consumed "name" + its value
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
                i += 1; // skip unrecognised token
            }
        }
    }

    // All three fields are mandatory.
    let name = name.ok_or_else(|| "Error: Missing parameter name.".to_string())?;
    let arrival = arrival.ok_or_else(|| "Error: Missing parameter arrival.".to_string())?;
    let burst = burst.ok_or_else(|| "Error: Missing parameter burst.".to_string())?;

    Ok(Process::new(name, arrival, burst))
}

// ---------------------------------------------------------------------------
// Simulation dispatcher
// ---------------------------------------------------------------------------

/// Run the simulation specified by config.algorithm and return an event log.
/// Each string in the returned Vec is one line of time-tick output.
fn simulate(config: &mut Config) -> Vec<String> {
    match &config.algorithm.clone() {
        Algorithm::Fcfs => simulate_fcfs(config),
        Algorithm::Sjf => simulate_sjf(config),
        Algorithm::Rr(q) => simulate_rr(config, *q),
    }
}

// ---------------------------------------------------------------------------
// FCFS scheduler
// ---------------------------------------------------------------------------

/// First-Come First-Served (non-preemptive).
///
/// Processes are sorted once by (arrival, name) and fed into a FIFO ready
/// queue. The CPU runs the current process to completion before picking the
/// next one. Wait time accumulates for every process sitting in the ready
/// queue each tick.
fn simulate_fcfs(config: &mut Config) -> Vec<String> {
    let run_for = config.run_for;
    let procs = &mut config.processes;

    // Stable sort ensures FIFO order for equal arrival times.
    procs.sort_by(|a, b| a.arrival.cmp(&b.arrival).then(a.name.cmp(&b.name)));

    let mut events: Vec<String> = Vec::new();
    let mut current: Option<usize> = None; // index of the currently running process
    let mut ready: VecDeque<usize> = VecDeque::new(); // indices in arrival order

    for t in 0..run_for {
        // --- Step 1: enqueue any processes arriving this tick ---
        for (i, p) in procs.iter().enumerate() {
            if p.arrival == t {
                events.push(format!("Time {:3} : {} arrived", t, p.name));
                ready.push_back(i);
            }
        }

        // --- Step 2: if CPU is free, pick the next ready process ---
        if current.is_none() {
            if let Some(idx) = ready.pop_front() {
                let p = &mut procs[idx];
                // Response time = first-selection tick minus arrival tick.
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

        // --- Step 3: advance simulation by one tick ---
        match current {
            None => {
                // No runnable process: CPU is idle this tick.
                events.push(format!("Time {:3} : Idle", t));
            }
            Some(idx) => {
                // Charge one tick of wait to every process in the ready queue.
                // We must collect indices first to avoid holding two mutable
                // borrows into `procs` at the same time (Rust's borrow checker
                // disallows indexing `procs` mutably while iterating it).
                let waiting: Vec<usize> = ready.iter().copied().collect();
                for ri in waiting {
                    procs[ri].wait += 1;
                }

                // Burn one tick of CPU on the running process.
                procs[idx].remaining -= 1;

                if procs[idx].remaining == 0 {
                    // Process finished: record completion stats.
                    let finish_t = t + 1;
                    procs[idx].finished = true;
                    procs[idx].turnaround = finish_t - procs[idx].arrival;
                    events.push(format!(
                        "Time {:3} : {} finished",
                        finish_t, procs[idx].name
                    ));
                    current = None; // CPU is now free
                }
            }
        }
    }

    events
}

// ---------------------------------------------------------------------------
// Preemptive SJF scheduler
// ---------------------------------------------------------------------------

/// Preemptive Shortest Job First (also called Shortest Remaining Time First).
///
/// Every tick, the ready process with the smallest `remaining` time is
/// selected. If a newly arrived process has a shorter remaining time than
/// the currently running one, it preempts it immediately.
/// Tie-breaking is alphabetical by name.
fn simulate_sjf(config: &mut Config) -> Vec<String> {
    let run_for = config.run_for;
    let procs = &mut config.processes;

    let mut events: Vec<String> = Vec::new();
    let mut ready: Vec<usize> = Vec::new(); // unordered pool of eligible indices
    let mut current: Option<usize> = None;

    for t in 0..run_for {
        // --- Step 1: arrivals ---
        for (i, p) in procs.iter().enumerate() {
            if p.arrival == t {
                events.push(format!("Time {:3} : {} arrived", t, p.name));
                ready.push(i);
            }
        }

        // --- Step 2: choose the best candidate from the ready pool ---
        // min_by gives us the index with the shortest remaining time
        // (alphabetical name used as a stable tie-breaker).
        let best = ready.iter().copied().min_by(|&a, &b| {
            procs[a]
                .remaining
                .cmp(&procs[b].remaining)
                .then(procs[a].name.cmp(&procs[b].name))
        });

        // --- Step 3: preempt or select ---
        match (current, best) {
            (None, Some(b)) => {
                // CPU was idle; select the best candidate.
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
                // A different process has a shorter remaining time: preempt.
                if procs[b].response.is_none() {
                    procs[b].response = Some(t - procs[b].arrival);
                }
                events.push(format!(
                    "Time {:3} : {} selected (burst {:3})",
                    t, procs[b].name, procs[b].remaining
                ));
                current = Some(b);
            }
            _ => {
                // Current process remains the best choice (or nothing to run).
            }
        }

        // --- Step 4: advance by one tick ---
        match current {
            None => {
                events.push(format!("Time {:3} : Idle", t));
            }
            Some(idx) => {
                // Charge one wait tick to every ready process except the running one.
                // Collect into a temporary Vec to satisfy the borrow checker —
                // iterating `ready` while also mutably indexing `procs` would
                // require two simultaneous mutable borrows of `procs`.
                let waiting: Vec<usize> = ready.iter().copied().filter(|&i| i != idx).collect();
                for ri in waiting {
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
                    // Remove the finished process from the ready pool.
                    ready.retain(|&i| i != idx);
                    current = None;
                }
            }
        }
    }

    events
}

// ---------------------------------------------------------------------------
// Round-Robin scheduler
// ---------------------------------------------------------------------------

/// Round-Robin with a fixed time quantum.
///
/// Processes are served in FIFO order from a circular ready queue.
/// Each process runs for at most `quantum` ticks before being preempted and
/// re-queued at the back. A process that completes before its quantum expires
/// simply releases the CPU early.
///
/// Newly arriving processes are enqueued *before* the quantum-expiry check so
/// that they become eligible in the same tick they arrive.
fn simulate_rr(config: &mut Config, quantum: u32) -> Vec<String> {
    let run_for = config.run_for;
    let procs = &mut config.processes;

    // Initial sort ensures predictable enqueue order for simultaneous arrivals.
    procs.sort_by(|a, b| a.arrival.cmp(&b.arrival).then(a.name.cmp(&b.name)));

    let mut events: Vec<String> = Vec::new();
    let mut ready: VecDeque<usize> = VecDeque::new();
    let mut current: Option<usize> = None;
    let mut quantum_left: u32 = 0; // ticks remaining in the current quantum slice

    for t in 0..run_for {
        // --- Step 1: enqueue newly arriving processes ---
        for (i, p) in procs.iter().enumerate() {
            if p.arrival == t {
                events.push(format!("Time {:3} : {} arrived", t, p.name));
                // Guard against double-adding (e.g. if the process is currently running).
                if current != Some(i) && !ready.contains(&i) {
                    ready.push_back(i);
                }
            }
        }

        // --- Step 2: check if the current quantum has expired ---
        // If so, re-queue the running process (if still unfinished) and clear current.
        if let Some(idx) = current {
            if quantum_left == 0 {
                if !procs[idx].finished {
                    ready.push_back(idx); // goes to the back of the queue
                }
                current = None;
            }
        }

        // --- Step 3: select next process if CPU is free ---
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
                quantum_left = quantum; // reset quantum counter for the new slice
            }
        }

        // --- Step 4: advance by one tick ---
        match current {
            None => {
                events.push(format!("Time {:3} : Idle", t));
            }
            Some(idx) => {
                // Charge one wait tick to every process in the ready queue.
                // Collect first to avoid simultaneous mutable borrows on `procs`.
                let waiting: Vec<usize> = ready.iter().copied().collect();
                for ri in waiting {
                    procs[ri].wait += 1;
                }

                procs[idx].remaining -= 1;
                quantum_left -= 1;

                if procs[idx].remaining == 0 {
                    // Process finished before (or exactly at) quantum expiry.
                    let finish_t = t + 1;
                    procs[idx].finished = true;
                    procs[idx].turnaround = finish_t - procs[idx].arrival;
                    events.push(format!(
                        "Time {:3} : {} finished",
                        finish_t, procs[idx].name
                    ));
                    current = None;
                    quantum_left = 0; // prevent the expiry check above from re-queuing
                }
            }
        }
    }

    events
}

// ---------------------------------------------------------------------------
// Output formatting
// ---------------------------------------------------------------------------

/// Build the complete output string from the simulation results.
///
/// Format:
///   <N> processes
///   Using <algorithm>
///   [Quantum <Q>]            <- only for RR
///   Time  X : <event> ...    <- one per tick with an event
///   Finished at time  Y
///
///   <name> wait W turnaround T response R   <- per process, sorted by name
///   <name> did not finish                   <- for processes that didn't complete
fn build_output(config: &Config, events: &[String]) -> String {
    let mut lines: Vec<String> = Vec::new();

    // Header: process count and algorithm name.
    lines.push(format!("{} processes", config.process_count));
    match &config.algorithm {
        Algorithm::Fcfs => lines.push("Using First-Come First-Served".to_string()),
        Algorithm::Sjf => lines.push("Using preemptive Shortest Job First".to_string()),
        Algorithm::Rr(q) => {
            lines.push("Using Round-Robin".to_string());
            lines.push(format!("Quantum {}", q));
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
