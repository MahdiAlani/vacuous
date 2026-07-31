//! Finds Python tests that pass no matter what the code does.
//!
//! Discover the files a test runner would collect, parse each one, pull out its
//! test functions, and run every check over them. Files go in parallel; the
//! report is sorted afterwards so output stays deterministic.

pub mod baseline;
pub mod discover;
pub mod lang;
pub mod output;
pub mod parse;
pub mod report;
pub mod rules;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rayon::prelude::*;

use lang::LanguageAdapter;
use report::Report;
use rules::{FileCtx, Rule, RuleCtx};

#[derive(Debug, Default)]
pub struct ScanOutcome {
    pub report: Report,
    /// Files we couldn't read or parse, with the reason. Reported rather than
    /// dropped: silently skipping half a suite is worse than not running.
    pub skipped: Vec<(PathBuf, String)>,
}

/// Scan `root` (a file or directory) for tests that cannot fail.
pub fn check(root: &Path, adapter: &dyn LanguageAdapter) -> Result<ScanOutcome> {
    let files = discover::find_test_files(root, adapter);
    let rules = rules::all_rules();
    let language = adapter.language();

    let results: Vec<std::result::Result<Report, (PathBuf, String)>> = files
        .par_iter()
        .map(|path| {
            scan_file(path, adapter, &rules, &language)
                .map_err(|e| (path.clone(), format!("{e:#}")))
        })
        .collect();

    let mut outcome = ScanOutcome::default();
    for result in results {
        match result {
            Ok(report) => outcome.report.absorb(report),
            Err(skipped) => outcome.skipped.push(skipped),
        }
    }
    outcome.report.sort();
    Ok(outcome)
}

fn scan_file(
    path: &Path,
    adapter: &dyn LanguageAdapter,
    rules: &[Box<dyn Rule>],
    language: &tree_sitter::Language,
) -> Result<Report> {
    let src =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let tree =
        parse::parse(language, &src).with_context(|| format!("parsing {}", path.display()))?;

    let root = tree.root_node();
    let tests = adapter.test_functions(root, &src);

    // Once per file, not once per test: this is linear in file size.
    let file_ctx = FileCtx {
        asserting_helpers: adapter.asserting_helpers(root, &src),
    };

    let mut report = Report {
        findings: Vec::new(),
        tests_scanned: tests.len(),
        files_scanned: 1,
        ..Default::default()
    };

    for test in &tests {
        let ctx = RuleCtx {
            src: &src,
            path,
            adapter,
            file: &file_ctx,
            test,
        };
        for rule in rules {
            report.findings.extend(rule.check(&ctx));
        }
    }

    Ok(report)
}
