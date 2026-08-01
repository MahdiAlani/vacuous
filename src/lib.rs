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

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rayon::prelude::*;

use lang::LanguageAdapter;
use report::Report;
use rules::{Helpers, Rule, RuleCtx};

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

    // First pass: which functions assert, across the whole suite rather than one
    // file at a time. A shared base class in a helper module is the normal way
    // to write assertion helpers, and resolving only within a file reports every
    // test that uses one as asserting nothing.
    let modules = discover::sibling_modules(&files);
    let helpers = Helpers {
        asserting: files
            .par_iter()
            .chain(modules.par_iter())
            .filter_map(|path| asserting_helpers(path, adapter, &language))
            .flatten()
            .collect(),
    };

    let results: Vec<std::result::Result<Report, (PathBuf, String)>> = files
        .par_iter()
        .map(|path| {
            scan_file(path, adapter, &rules, &language, &helpers)
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

/// Functions in one file that assert, directly or through another local call.
/// Failures are ignored: an unreadable helper module shouldn't fail the scan.
fn asserting_helpers(
    path: &Path,
    adapter: &dyn LanguageAdapter,
    language: &tree_sitter::Language,
) -> Option<HashSet<String>> {
    let src = std::fs::read_to_string(path).ok()?;
    let tree = parse::parse(language, &src).ok()?;
    Some(adapter.asserting_helpers(tree.root_node(), &src))
}

fn scan_file(
    path: &Path,
    adapter: &dyn LanguageAdapter,
    rules: &[Box<dyn Rule>],
    language: &tree_sitter::Language,
    helpers: &Helpers,
) -> Result<Report> {
    let src =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let tree =
        parse::parse(language, &src).with_context(|| format!("parsing {}", path.display()))?;

    let root = tree.root_node();
    let tests = adapter.test_functions(root, &src);

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
            helpers,
            test,
        };
        for rule in rules {
            report.findings.extend(rule.check(&ctx));
        }
    }

    Ok(report)
}
