use std::env;
use std::fs;
use std::path::Path;

mod models;
mod output;
mod parser;
mod scheduler;

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
    let mut config = match parser::parse_input(&content) {
        Ok(c) => c,
        Err(e) => exit_with_mesg!("{}", e),
    };

    // Run the chosen scheduling algorithm and collect the event log.
    let events = scheduler::simulate(&mut config);

    // Format the full output text from the event log and process stats.
    let output = output::build_output(&config, &events);

    // Derive the output filename: take only the file stem (no directory
    // component) and add ".out", then write into the current working directory.
    // e.g. running with "../tests/data/input.in" produces "./input.out"
    let out_filename = Path::new(input_path)
        .file_stem()                         // "input"  (drops directory + extension)
        .expect("input path has no filename")
        .to_string_lossy()
        .to_string()
        + ".out";                            // -> "input.out"
    let out_path = Path::new(&out_filename).to_path_buf();

    // Write output file; exit with an error if the write fails.
    if let Err(e) = fs::write(&out_path, &output) {
        eprintln!("Error writing '{}': {}", out_path.display(), e);
        std::process::exit(1);
    }

    println!("Output written to {}", out_path.display());
}
