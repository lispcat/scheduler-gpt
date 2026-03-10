use std::fs;
use std::path::Path;

use clap::Parser;

mod args;
mod models;
mod output;
mod parser;
mod scheduler;
mod tui;

fn main() {
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
    let mut sim_config = match parser::parse_input(&content) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    // Run the simulation.
    let events = scheduler::simulate(&mut sim_config);

    // Build plain output (no ANSI codes) — used for the .out file and as the
    // base for non-colorized stdout.
    let plain = output::build_output(&sim_config, &events, false);

    if args.tui {
        // TUI mode: write the .out file first so it exists even if the user
        // quits the TUI early, then open the interactive interface.
        // -p / -d / -c are ignored in TUI mode.
        write_output_file(input_path, &plain);
        let input_path_str = input_path.to_string_lossy();
        if let Err(e) = tui::run_tui(&input_path_str, &content, &sim_config, &plain) {
            eprintln!("TUI error: {}", e);
            std::process::exit(1);
        }
    } else {
        // Normal (non-TUI) mode.

        // Write .out file unless -d / --no-file was passed.
        if !args.no_file {
            write_output_file(input_path, &plain);
        }

        // Print to stdout if -p / --print was passed.
        if args.print {
            let display = output::build_output(&sim_config, &events, args.color);
            print!("{}", display);
        }
    }
}

/// Derive the output path from the input path and write `content` to it.
/// The output is always placed in the current working directory, never next
/// to the input file (which may be in a different directory).
fn write_output_file(input_path: &Path, content: &str) {
    let out_filename = input_path
        .file_stem()
        .expect("input path has no filename")
        .to_string_lossy()
        .to_string()
        + ".out";
    let out_path = Path::new(&out_filename).to_path_buf();

    if let Err(e) = fs::write(&out_path, content) {
        eprintln!("Error writing '{}': {}", out_path.display(), e);
        std::process::exit(1);
    }

    eprintln!("Output written to {}", out_path.display());
}
