use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand, ValueEnum};

use vacuous::baseline::{self, Baseline};
use vacuous::lang::python::Python;
use vacuous::output;
use vacuous::report::{self, Confidence};

#[derive(Parser)]
#[command(
    name = "vacuous",
    version,
    about = "Find Python tests that pass no matter what your code does",
    long_about = "Finds tests with no assertions, assertions on literals, \
                  assertions that get discarded, and tests that only check their \
                  own mocks.\n\n\
                  Static analysis only: it never runs your tests."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scan test files for tests that cannot fail.
    Check {
        /// File or directory to scan.
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Lowest confidence to report: certain, likely, or possible.
        #[arg(long, default_value = "likely", value_name = "LEVEL")]
        min_confidence: String,

        #[arg(long, value_enum, default_value_t = Format::Pretty)]
        format: Format,

        /// Baseline to ignore. Defaults to .vacuous-baseline.json if present.
        #[arg(long, value_name = "FILE")]
        baseline: Option<PathBuf>,

        /// Report everything, even findings the baseline covers.
        #[arg(long, conflicts_with = "baseline")]
        no_baseline: bool,
    },

    /// Record the current findings so `check` only reports new ones.
    Baseline {
        /// File or directory to scan.
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Matches `check`, so the baseline covers what `check` would report.
        #[arg(long, default_value = "likely", value_name = "LEVEL")]
        min_confidence: String,

        /// Where to write. Defaults to .vacuous-baseline.json in the scan root.
        #[arg(long, value_name = "FILE")]
        output: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Format {
    Pretty,
    Json,
    /// For GitHub code scanning, which turns findings into PR annotations.
    Sarif,
}

/// 0 clean, 1 findings, 2 the tool itself broke. CI needs to tell those apart.
fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::from(1),
        Ok(false) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("vacuous: {err:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<bool> {
    match Cli::parse().command {
        Command::Check {
            path,
            min_confidence,
            format,
            baseline,
            no_baseline,
        } => check(path, &min_confidence, format, baseline, no_baseline),

        Command::Baseline {
            path,
            min_confidence,
            output,
        } => write_baseline(path, &min_confidence, output),
    }
}

fn check(
    path: PathBuf,
    min_confidence: &str,
    format: Format,
    baseline_path: Option<PathBuf>,
    no_baseline: bool,
) -> Result<bool> {
    let min = parse_confidence(min_confidence)?;
    let mut outcome = scan(&path, min)?;

    if !no_baseline {
        // An explicit --baseline that doesn't exist is a mistake worth
        // reporting. A missing default just means there isn't one yet.
        let explicit = baseline_path.is_some();
        let file = baseline_path.unwrap_or_else(|| default_baseline_path(&path));

        if file.exists() {
            let applied = Baseline::load(&file)?.apply(&mut outcome.report, &path);
            if applied.stale > 0 && format == Format::Pretty {
                eprintln!(
                    "vacuous: {} baseline {} no longer occur; re-run `vacuous baseline` to tidy up",
                    applied.stale,
                    if applied.stale == 1 {
                        "entry"
                    } else {
                        "entries"
                    }
                );
            }
        } else if explicit {
            return Err(anyhow!("baseline not found: {}", file.display()));
        }
    }

    match format {
        Format::Pretty => report::print_pretty(&outcome.report, &path),
        Format::Json => println!("{}", output::to_json(&outcome.report, &path)),
        Format::Sarif => println!("{}", output::to_sarif(&outcome.report, &path)),
    }

    // stderr, so it can't corrupt JSON on stdout.
    for (file, reason) in &outcome.skipped {
        eprintln!("vacuous: skipped {}: {reason}", file.display());
    }

    Ok(!outcome.report.findings.is_empty())
}

fn write_baseline(path: PathBuf, min_confidence: &str, output: Option<PathBuf>) -> Result<bool> {
    let min = parse_confidence(min_confidence)?;
    let outcome = scan(&path, min)?;

    let file = output.unwrap_or_else(|| default_baseline_path(&path));
    let baseline = Baseline::from_report(&outcome.report, &path);
    baseline.save(&file)?;

    eprintln!(
        "vacuous: recorded {} {} in {}",
        baseline.entries.len(),
        if baseline.entries.len() == 1 {
            "finding"
        } else {
            "findings"
        },
        file.display()
    );
    eprintln!("vacuous: `vacuous check` will now report only new ones");

    // Writing a baseline succeeded, so this is not a failure.
    Ok(false)
}

fn scan(path: &Path, min: Confidence) -> Result<vacuous::ScanOutcome> {
    if !path.exists() {
        return Err(anyhow!("path does not exist: {}", path.display()));
    }
    let mut outcome = vacuous::check(path, &Python)?;
    outcome.report.retain_at_least(min);
    Ok(outcome)
}

fn parse_confidence(value: &str) -> Result<Confidence> {
    Confidence::parse(value).ok_or_else(|| {
        anyhow!("invalid confidence `{value}` (expected `certain`, `likely`, or `possible`)")
    })
}

/// Beside the code being scanned, so it travels with the repo.
fn default_baseline_path(root: &Path) -> PathBuf {
    if root.is_dir() {
        root.join(baseline::DEFAULT_FILE)
    } else {
        PathBuf::from(baseline::DEFAULT_FILE)
    }
}
