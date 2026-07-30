//! Finding test files on disk.

use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use crate::lang::LanguageAdapter;

/// Collect every test file under `root`.
///
/// Uses the `ignore` crate so `.gitignore` is respected for free — without
/// this we would happily scan `.venv/` and `node_modules/` and report on
/// third-party test suites.
///
/// If `root` is a single file, it is returned as-is without the test-name
/// check, so `vacuous check some_file.py` always does what the user meant.
pub fn find_test_files(root: &Path, adapter: &dyn LanguageAdapter) -> Vec<PathBuf> {
    if root.is_file() {
        return vec![root.to_path_buf()];
    }

    let mut files: Vec<PathBuf> = WalkBuilder::new(root)
        .hidden(false) // don't skip dotted dirs wholesale; .gitignore still applies
        .build()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_some_and(|t| t.is_file()))
        .map(|entry| entry.into_path())
        .filter(|path| adapter.is_test_file(path))
        .collect();

    // Deterministic order regardless of filesystem traversal order.
    files.sort();
    files
}
