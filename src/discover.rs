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

/// Python modules sitting beside the tests, which no runner collects.
///
/// Test suites keep their shared assertion helpers here: pyflakes asserts
/// through `self.flakes()`, defined in `pyflakes/test/harness.py`. Without
/// reading those, every test in the suite looks like it asserts nothing.
///
/// Only the directories that actually hold tests, rather than every `.py` file
/// in the project, since helpers essentially always live next to their tests
/// and walking a whole source tree would cost more than it finds.
pub fn sibling_modules(test_files: &[PathBuf]) -> Vec<PathBuf> {
    let mut directories: Vec<&Path> = test_files.iter().filter_map(|f| f.parent()).collect();
    directories.sort();
    directories.dedup();

    let known: std::collections::HashSet<&Path> = test_files.iter().map(PathBuf::as_path).collect();

    let mut modules: Vec<PathBuf> = directories
        .into_iter()
        .filter_map(|directory| std::fs::read_dir(directory).ok())
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path.extension().and_then(|e| e.to_str()) == Some("py")
                && !known.contains(path.as_path())
        })
        .collect();

    modules.sort();
    modules
}
