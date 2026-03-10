use std::fs;
use std::path::Path;

use clap::Parser;

mod args;
mod models;
mod output;
mod parser;
mod scheduler;

fn main() {
    // Parse CLI arguments via Clap.  --help / -h and --version are provided
    // automatically by Clap; invalid usage prints an error and exits.
    let args = args::Args::parse();

    let input_path = &args.input_file;

    // Read the entire input file into a String.
    let content = match fs::read_to_string(input_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading '{}': {}", input_path.display(), e);
            std::process::exit(1);
        }
    };

    // Parse the input text into a SimConfig (domain config from the .in file).
    // Any structural errors (missing directives, bad values) cause an early exit.
    let mut sim_config = match parser::parse_input(&content) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    // Run the chosen scheduling algorithm and collect the event log.
    let events = scheduler::simulate(&mut sim_config);

    // Build plain-text output for the .out file (never contains ANSI codes).
    let plain = output::build_output(&sim_config, &events, false);

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

    if let Err(e) = fs::write(&out_path, &plain) {
        eprintln!("Error writing '{}': {}", out_path.display(), e);
        std::process::exit(1);
    }

    // When --color is requested, print a colorized version to stdout.
    // Otherwise print the same plain text (mirrors the file contents).
    let display = output::build_output(&sim_config, &events, args.color);
    print!("{}", display);

    eprintln!("Output written to {}", out_path.display());
}
