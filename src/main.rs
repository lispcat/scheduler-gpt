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

    // Build plain output (always written to the .out file).
    let plain = output::build_output(&sim_config, &events, false);

    // Derive output file path and write it.
    let out_filename = Path::new(input_path)
        .file_stem()
        .expect("input path has no filename")
        .to_string_lossy()
        .to_string()
        + ".out";
    let out_path = Path::new(&out_filename).to_path_buf();

    if let Err(e) = fs::write(&out_path, &plain) {
        eprintln!("Error writing '{}': {}", out_path.display(), e);
        std::process::exit(1);
    }

    if args.tui {
        // TUI mode: confirmation screen -> results screen.
        // The output file has already been written above, so the user can
        // inspect it even if they quit the TUI early.
        let input_path_str = input_path.to_string_lossy();
        if let Err(e) = tui::run_tui(&input_path_str, &content, &sim_config, &plain) {
            eprintln!("TUI error: {}", e);
            std::process::exit(1);
        }
    } else {
        // Normal (non-TUI) mode: print to stdout, optionally colorized.
        let display = output::build_output(&sim_config, &events, args.color);
        print!("{}", display);
        eprintln!("Output written to {}", out_path.display());
    }
}
