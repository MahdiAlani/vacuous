//! vacuous — find the tests that cannot fail.
//!
//! ## How a scan works
//!
//! 1. [`discover::find_test_files`] walks the tree for files a test runner
//!    would collect.
//! 2. Each file is parsed with tree-sitter and handed to the
//!    [`lang::LanguageAdapter`] to extract its test functions.
//! 3. Every [`rules::Rule`] runs against every test function.
//! 4. Findings are merged, sorted, and filtered by confidence.
//!
//! Files are scanned in parallel; everything is deterministic because the
//! final report is sorted by location.

pub mod discover;
pub mod lang;
pub mod parse;
pub mod report;
pub mod rules;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rayon::prelude::*;

use lang::LanguageAdapter;
use report::Report;
use rules::{FileCtx, Rule, RuleCtx};

/// The result of a scan, including files we could not handle.
#[derive(Debug, Default)]
pub struct ScanOutcome {
    pub report: Report,
    /// Files that could not be read or parsed, with the reason. Surfaced to the
    /// user rather than silently dropped — a scan that quietly skipped half the
    /// suite would be worse than no scan at all.
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

    // Computed once per file, not once per test — resolving the call graph is
    // linear in file size.
    let file_ctx = FileCtx {
        asserting_helpers: adapter.asserting_helpers(root, &src),
    };

    let mut report = Report {
        findings: Vec::new(),
        tests_scanned: tests.len(),
        files_scanned: 1,
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
