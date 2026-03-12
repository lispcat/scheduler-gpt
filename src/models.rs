use std::fmt;

// ---------------------------------------------------------------------------
// Process
// ---------------------------------------------------------------------------

/// A single process, including both its static definition and mutable
/// simulation state that gets updated as the scheduler runs.
#[derive(Debug, Clone)]
pub struct Process {
    // --- Static fields (set at parse time, never change) ---
    pub name: String, // process identifier, e.g. "A"
    pub arrival: u32, // time tick at which this process enters the system
    pub burst: u32,   // total CPU time needed to complete

    // --- Mutable simulation fields (updated during simulation) ---
    pub remaining: u32,        // CPU time still needed (counts down from `burst`)
    pub wait: u32,             // total time spent in ready queue (not running)
    pub turnaround: u32,       // total time from arrival to completion (finish - arrival)
    pub response: Option<u32>, // time from arrival until first CPU assignment; None until set
    pub finished: bool,        // true once remaining reaches 0
    pub started: bool,         // true once the process has been selected at least once
}

impl Process {
    /// Construct a new process with fully initialised simulation state.
    pub fn new(name: String, arrival: u32, burst: u32) -> Self {
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

// ---------------------------------------------------------------------------
// Algorithm
// ---------------------------------------------------------------------------

/// The three supported scheduling algorithms.
/// RR carries its quantum length as an associated value so it is always
/// available wherever an Algorithm value is used.
#[derive(Debug, Clone, PartialEq)]
pub enum Algorithm {
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
            // output::build_output prints it on a separate "Quantum N" line.
            Algorithm::Rr(_q) => write!(f, "Round-Robin"),
        }
    }
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// All configuration parsed from the input file.
#[derive(Debug)]
pub struct Config {
    pub process_count: usize, // declared count of processes (from `processcount` directive)
    pub run_for: u32,         // total simulation duration in time ticks
    pub algorithm: Algorithm, // which scheduling algorithm to use
    pub processes: Vec<Process>, // list of all processes
}
