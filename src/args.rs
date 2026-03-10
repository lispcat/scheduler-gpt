use std::path::PathBuf;

use clap::Parser;

/// Process scheduling algorithm simulator.
///
/// Reads an input file describing processes and a scheduling algorithm,
/// runs the simulation, and writes the results to <input>.out in the
/// current working directory.
#[derive(Parser, Debug)]
#[command(
    name = "scheduler-get",
    version,
    about,
    override_usage = "scheduler-get [OPTIONS] <input file>"
)]
pub struct Args {
    /// Path to the .in input file describing the workload.
    pub input_file: PathBuf,

    /// Colorize output with ANSI escape codes.
    ///
    /// Events are color-coded by type:
    ///   arrived  -> cyan
    ///   selected -> green
    ///   finished -> yellow
    ///   Idle     -> dark grey
    #[arg(short = 'c', long = "color", default_value_t = false)]
    pub color: bool,

    /// Open an interactive TUI to preview input and view results.
    ///
    /// Two screens:
    ///   1. Confirmation: shows the input path, file contents, and parsed
    ///      summary.  Press <Enter> to run the simulation.
    ///   2. Results: shows the simulation output.
    ///      Press <Enter> or <q> to exit.
    #[arg(short = 't', long = "tui", default_value_t = false)]
    pub tui: bool,
}
