// tests/integration_test.rs
//
// Integration tests for the scheduler binary.
//
// Each test:
//   1. Locates the compiled binary via `env!("CARGO_BIN_EXE_<name>")`.
//   2. Creates a temporary directory and sets it as the working directory
//      for the child process, so the produced ".out" file lands there and
//      never overwrites the golden reference files under tests/data/.
//   3. Runs the binary with the path to a ".in" file under tests/data/.
//   4. Reads the produced output from the temp dir and the golden reference
//      from tests/data/, compares them, and prints a line-level diff on
//      mismatch so failures are easy to diagnose.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Absolute path to the tests/data directory, resolved relative to the
/// location of this source file at *compile* time (CARGO_MANIFEST_DIR).
fn data_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
}

/// Run the scheduler binary with `input_file` as the sole argument, with
/// `cwd` as the working directory.  Returns (exit_status, stdout, stderr).
fn run_scheduler(input_file: &Path, cwd: &Path) -> (std::process::ExitStatus, String, String) {
    // `env!("CARGO_BIN_EXE_scheduler")` is injected by Cargo at compile time;
    // the string after the underscore must match the [[bin]] name in Cargo.toml.
    let bin = env!("CARGO_BIN_EXE_scheduler-get");

    let output = Command::new(bin)
        .arg(input_file)
        .current_dir(cwd)
        .output()
        .expect("failed to launch scheduler binary");

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (output.status, stdout, stderr)
}

/// Produce a simple line-level unified-style diff string between `expected`
/// and `actual`.  Only changed / added / removed lines are shown; context
/// lines are omitted to keep output concise.
fn simple_diff(expected: &str, actual: &str) -> String {
    let exp_lines: Vec<&str> = expected.lines().collect();
    let act_lines: Vec<&str> = actual.lines().collect();

    let max = exp_lines.len().max(act_lines.len());
    let mut diff = String::new();

    for i in 0..max {
        match (exp_lines.get(i), act_lines.get(i)) {
            (Some(e), Some(a)) if e == a => {
                // Lines match — skip (no context lines)
            }
            (Some(e), Some(a)) => {
                diff.push_str(&format!("line {:>3}  expected: {}\n", i + 1, e));
                diff.push_str(&format!("         actual  : {}\n", a));
            }
            (Some(e), None) => {
                diff.push_str(&format!("line {:>3}  expected: {}\n", i + 1, e));
                diff.push_str("         actual  : <missing>\n");
            }
            (None, Some(a)) => {
                diff.push_str(&format!("line {:>3}  expected: <missing>\n", i + 1));
                diff.push_str(&format!("         actual  : {}\n", a));
            }
            (None, None) => unreachable!(),
        }
    }

    diff
}

/// Core test helper.  Runs the binary against `<stem>.in`, then compares the
/// produced `<stem>.out` (in the temp dir) against `tests/data/<stem>.out`.
fn run_test(stem: &str) {
    let data = data_dir();
    let input_file = data.join(format!("{}.in", stem));
    let golden_file = data.join(format!("{}.out", stem));

    // Sanity-check that the test fixtures actually exist.
    assert!(
        input_file.exists(),
        "test input not found: {}",
        input_file.display()
    );
    assert!(
        golden_file.exists(),
        "golden output not found: {}",
        golden_file.display()
    );

    // Create a temp directory; the binary will write its output here.
    let tmp = tempfile(stem);
    let produced_file = tmp.join(format!("{}.out", stem));

    // Run the binary.
    let (status, stdout, stderr) = run_scheduler(&input_file, &tmp);

    assert!(
        status.success(),
        "scheduler exited with non-zero status\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );

    // The produced output file must exist.
    assert!(
        produced_file.exists(),
        "scheduler did not produce output file: {}\nstdout: {}\nstderr: {}",
        produced_file.display(),
        stdout,
        stderr
    );

    let actual = fs::read_to_string(&produced_file).expect("could not read produced output file");
    let expected = fs::read_to_string(&golden_file).expect("could not read golden output file");

    if actual != expected {
        let diff = simple_diff(&expected, &actual);
        panic!(
            "output mismatch for '{}':\n{}\n--- expected: {}\n+++ actual:   {}",
            stem,
            diff,
            golden_file.display(),
            produced_file.display(),
        );
    }
    // Success — temp dir is dropped and cleaned up automatically.
}

/// Create a uniquely-named temp directory under the system temp root.
/// Returns the path; the caller owns it and it is deleted when dropped via
/// the `TempDir` wrapper below.  We use a plain PathBuf here so the test
/// body stays simple.
fn tempfile(label: &str) -> PathBuf {
    // Build a unique-ish path using the test label + process id.
    let dir = env::temp_dir().join(format!("scheduler_test_{}_{}", label, std::process::id()));
    fs::create_dir_all(&dir).expect("could not create temp directory");
    dir
}

// ---------------------------------------------------------------------------
// Test cases - one function per input/output fixture pair in tests/data/
// ---------------------------------------------------------------------------

#[test]
fn test_c10_fcfs() {
    run_test("c10-fcfs");
}

#[test]
fn test_c10_rr() {
    run_test("c10-rr");
}

#[test]
fn test_c10_sjf() {
    run_test("c10-sjf");
}

#[test]
fn test_c2_fcfs() {
    run_test("c2-fcfs");
}

#[test]
fn test_c2_rr() {
    run_test("c2-rr");
}

#[test]
fn test_c2_sjf() {
    run_test("c2-sjf");
}

#[test]
fn test_c5_fcfs() {
    run_test("c5-fcfs");
}

#[test]
fn test_c5_rr() {
    run_test("c5-rr");
}

#[test]
fn test_c5_sjf() {
    run_test("c5-sjf");
}

// Add more tests here as you add fixture files, e.g.:
// #[test]
// fn test_c3_fcfs() { run_test("c3-fcfs"); }
