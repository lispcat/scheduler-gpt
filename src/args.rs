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
    // Override the default usage line so it matches the spec format.
    override_usage = "scheduler-get [OPTIONS] <input file>"
)]
pub struct Args {
    /// Path to the .in input file describing the workload.
    pub input_file: PathBuf,

    /// Colorize output with ANSI escape codes.
    ///
    /// Events are color-coded by type:
    ///   arrived  → cyan
    ///   selected → green
    ///   finished → yellow
    ///   Idle     → dark grey
    ///   stats    → default (no color)
    #[arg(short = 'c', long = "color", default_value_t = false)]
    pub color: bool,
}
