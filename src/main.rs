use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};

use vacuous::lang::python::Python;
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
    },
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
    let cli = Cli::parse();

    match cli.command {
        Command::Check {
            path,
            min_confidence,
        } => {
            let min = Confidence::parse(&min_confidence).ok_or_else(|| {
                anyhow!(
                    "invalid --min-confidence `{min_confidence}` \
                     (expected `certain`, `likely`, or `possible`)"
                )
            })?;

            if !path.exists() {
                return Err(anyhow!("path does not exist: {}", path.display()));
            }

            let mut outcome = vacuous::check(&path, &Python)?;
            outcome.report.retain_at_least(min);
            report::print_pretty(&outcome.report, &path);

            for (file, reason) in &outcome.skipped {
                eprintln!("vacuous: skipped {}: {reason}", file.display());
            }

            Ok(!outcome.report.findings.is_empty())
        }
    }
}
