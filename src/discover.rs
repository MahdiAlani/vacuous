//! Finding test files on disk.

use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use crate::lang::LanguageAdapter;

/// Every test file under `root`.
///
/// The `ignore` crate gives us `.gitignore` handling, which keeps us out of
/// `.venv/` and other people's test suites.
///
/// A `root` that's already a file comes back as-is, skipping the name check, so
/// `vacuous check some_file.py` does the obvious thing.
pub fn find_test_files(root: &Path, adapter: &dyn LanguageAdapter) -> Vec<PathBuf> {
    if root.is_file() {
        return vec![root.to_path_buf()];
    }

    let mut files: Vec<PathBuf> = WalkBuilder::new(root)
        // Don't skip dotted dirs wholesale; .gitignore still applies.
        .hidden(false)
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
