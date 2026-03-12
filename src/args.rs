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

    /// Colorize output with ANSI escape codes (non-TUI mode only).
    ///
    /// Events are color-coded by type:
    ///   arrived  -> cyan
    ///   selected -> green
    ///   finished -> yellow
    ///   Idle     -> dark grey
    #[arg(short = 'c', long = "color", default_value_t = false)]
    pub color: bool,

    /// Print output to stdout (non-TUI mode only).
    ///
    /// Does not affect whether the .out file is written; combine with -d to
    /// suppress the file entirely.
    #[arg(short = 'p', long = "print", default_value_t = false)]
    pub print: bool,

    /// Disable writing the .out file.
    ///
    /// Works in both normal and TUI mode.  In TUI mode the simulation still
    /// runs; results are shown on screen but nothing is written to disk.
    #[arg(short = 'd', long = "no-file", default_value_t = false)]
    pub no_file: bool,

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
