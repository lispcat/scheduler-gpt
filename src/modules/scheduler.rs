use std::collections::VecDeque;

use crate::modules::models::{Algorithm, Config};

// ---------------------------------------------------------------------------
// Public dispatcher
// ---------------------------------------------------------------------------

/// Run the simulation specified by config.algorithm and return an event log.
/// Each string in the returned Vec is one line of time-tick output.
pub fn simulate(config: &mut Config) -> Vec<String> {
    match &config.algorithm.clone() {
        Algorithm::Fcfs => simulate_fcfs(config),
        Algorithm::Sjf => simulate_sjf(config),
        Algorithm::Rr(q) => simulate_rr(config, *q),
    }
}

// ---------------------------------------------------------------------------
// FCFS
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
                                                      // A process finishing at end of tick t gets a "finished" stamp of t+1.
                                                      // We defer emitting that event so that arrivals at t+1 appear first in
                                                      // the log, matching the expected output ordering.
    let mut pending_finish: Option<usize> = None;

    for t in 0..run_for {
        // --- Step 1: enqueue any processes arriving this tick ---
        for (i, p) in procs.iter().enumerate() {
            if p.arrival == t {
                events.push(format!("Time {:3} : {} arrived", t, p.name));
                ready.push_back(i);
            }
        }

        // --- Step 2: emit finish event deferred from the previous tick ---
        // Arrivals are emitted first so that when a finish and an arrival share
        // the same timestamp, the arrival appears first in the log.
        if let Some(idx) = pending_finish.take() {
            events.push(format!("Time {:3} : {} finished", t, procs[idx].name));
        }

        // --- Step 3: if CPU is free, pick the next ready process ---
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

        // --- Step 4: advance simulation by one tick ---
        match current {
            None => {
                // No runnable process: CPU is idle this tick.
                events.push(format!("Time {:3} : Idle", t));
            }
            Some(idx) => {
                // Charge one tick of wait to every process in the ready queue.
                // Collect indices first to avoid simultaneous mutable borrows.
                let waiting: Vec<usize> = ready.iter().copied().collect();
                for ri in waiting {
                    procs[ri].wait += 1;
                }

                // Burn one tick of CPU on the running process.
                procs[idx].remaining -= 1;

                if procs[idx].remaining == 0 {
                    procs[idx].finished = true;
                    procs[idx].turnaround = (t + 1) - procs[idx].arrival;
                    pending_finish = Some(idx); // emit at start of next tick
                    current = None;
                }
            }
        }
    }

    // Flush any finish event that falls exactly on the run_for boundary.
    if let Some(idx) = pending_finish.take() {
        events.push(format!("Time {:3} : {} finished", run_for, procs[idx].name));
    }

    events
}

// ---------------------------------------------------------------------------
// Preemptive SJF
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
    let mut pending_finish: Option<usize> = None; // deferred finish event (see FCFS)

    for t in 0..run_for {
        // --- Step 1: arrivals ---
        for (i, p) in procs.iter().enumerate() {
            if p.arrival == t {
                events.push(format!("Time {:3} : {} arrived", t, p.name));
                ready.push(i);
            }
        }

        // --- Step 2: emit finish event deferred from the previous tick ---
        // Arrivals are emitted first so that when a finish and an arrival share
        // the same timestamp, the arrival appears first in the log.
        if let Some(idx) = pending_finish.take() {
            events.push(format!("Time {:3} : {} finished", t, procs[idx].name));
        }

        // --- Step 3: choose the best candidate from the ready pool ---
        // min_by gives us the index with the shortest remaining time
        // (alphabetical name used as a stable tie-breaker).
        let best = ready.iter().copied().min_by(|&a, &b| {
            procs[a]
                .remaining
                .cmp(&procs[b].remaining)
                .then(procs[a].name.cmp(&procs[b].name))
        });

        // --- Step 4: preempt or select ---
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

        // --- Step 5: advance by one tick ---
        match current {
            None => {
                events.push(format!("Time {:3} : Idle", t));
            }
            Some(idx) => {
                // Charge one wait tick to every ready process except the running one.
                let waiting: Vec<usize> = ready.iter().copied().filter(|&i| i != idx).collect();
                for ri in waiting {
                    procs[ri].wait += 1;
                }

                procs[idx].remaining -= 1;

                if procs[idx].remaining == 0 {
                    procs[idx].finished = true;
                    procs[idx].turnaround = (t + 1) - procs[idx].arrival;
                    ready.retain(|&i| i != idx);
                    pending_finish = Some(idx); // emit at start of next tick
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

// ---------------------------------------------------------------------------
// Round-Robin
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
    let mut pending_finish: Option<usize> = None; // deferred finish event (see FCFS)

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

        // --- Step 2: emit finish event deferred from the previous tick ---
        // Arrivals are emitted first so that when a finish and an arrival share
        // the same timestamp, the arrival appears first in the log.
        if let Some(idx) = pending_finish.take() {
            events.push(format!("Time {:3} : {} finished", t, procs[idx].name));
        }

        // --- Step 3: check if the current quantum has expired ---
        // If so, re-queue the running process (if still unfinished) and clear current.
        if let Some(idx) = current {
            if quantum_left == 0 {
                if !procs[idx].finished {
                    ready.push_back(idx); // goes to the back of the queue
                }
                current = None;
            }
        }

        // --- Step 4: select next process if CPU is free ---
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

        // --- Step 5: advance by one tick ---
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
                    procs[idx].finished = true;
                    procs[idx].turnaround = (t + 1) - procs[idx].arrival;
                    pending_finish = Some(idx); // emit at start of next tick
                    current = None;
                    quantum_left = 0; // prevent the expiry check above from re-queuing
                }
            }
        }
    }

    if let Some(idx) = pending_finish.take() {
        events.push(format!("Time {:3} : {} finished", run_for, procs[idx].name));
    }

    events
}
