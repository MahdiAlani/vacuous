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
    about = "Find the tests that cannot fail",
    long_about = "vacuous finds tests that pass no matter what the code does — \
                  tests with no assertions, tautological assertions, or assertions \
                  that only verify their own mocks.\n\n\
                  Everything runs locally. No network, no API key, no LLM."
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

        /// Lowest confidence to report: `certain`, `likely`, or `possible`.
        ///
        /// Defaults to `likely` so that speculative heuristics never cause a
        /// false accusation on a first run.
        #[arg(long, default_value = "likely", value_name = "LEVEL")]
        min_confidence: String,
    },
}

/// Exit codes: 0 = clean, 1 = findings, 2 = the tool itself failed.
/// CI can therefore distinguish "your tests are bad" from "vacuous is broken".
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
