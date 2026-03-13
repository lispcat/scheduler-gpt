use std::fs;
use std::path::Path;

use clap::Parser;

mod modules;

use modules::args::Args;
use modules::output::build_output;
use modules::parser::parse_input;
use modules::scheduler::simulate;
use modules::tui::run_tui;

fn main() {
    let args = Args::parse();

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
    let mut sim_config = match parse_input(&content) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    // Run the simulation.
    let events = simulate(&mut sim_config);

    // Build plain output (no ANSI codes) — used for the .out file.
    let plain = build_output(&sim_config, &events, false);

    // Write the .out file unless -d was passed.  This applies in all modes.
    if !args.no_file {
        write_output_file(input_path, &plain);
    }

    if args.tui {
        // TUI mode: -c and -p are ignored; -d was already honoured above.
        let input_path_str = input_path.to_string_lossy();
        if let Err(e) = run_tui(&input_path_str, &content, &sim_config, &plain) {
            eprintln!("TUI error: {}", e);
            std::process::exit(1);
        }
    } else {
        // Normal mode: print to stdout if -p was passed.
        if args.print {
            let display = build_output(&sim_config, &events, args.color);
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
