use crate::models::{Algorithm, Config, Process};

// ---------------------------------------------------------------------------
// Public API
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
pub fn parse_input(content: &str) -> Result<Config, String> {
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

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

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
