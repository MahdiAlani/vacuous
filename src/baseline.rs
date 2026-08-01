//! Accepting the findings a project already has.
//!
//! Running this on an established codebase turns up hundreds of results, and
//! nobody is going to fix those before their next commit. A baseline records
//! what exists today so CI only fails on new ones.
//!
//! Entries deliberately carry no line number. Line numbers move every time
//! anyone edits a file above the test, and a baseline that goes stale on every
//! commit is worse than none.

use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::report::{Report, relative_path};

pub const DEFAULT_FILE: &str = ".vacuous-baseline.json";

#[derive(Debug, Serialize, Deserialize)]
pub struct Baseline {
    pub version: u32,
    pub generated_by: String,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Entry {
    pub rule: String,
    pub file: String,
    pub test: String,
}

/// What applying a baseline did, so the summary can say so out loud.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Applied {
    pub suppressed: usize,
    /// Recorded but no longer found, usually because someone fixed it.
    pub stale: usize,
}

impl Baseline {
    pub fn from_report(report: &Report, root: &Path) -> Self {
        let mut entries: Vec<Entry> = report
            .findings
            .iter()
            .map(|f| Entry {
                rule: f.rule.to_string(),
                file: relative_path(&f.file, root),
                test: f.test_name.clone(),
            })
            .collect();

        // Sorted and deduplicated so the file diffs cleanly between runs.
        entries.sort();
        entries.dedup();

        Self {
            version: 1,
            generated_by: format!("vacuous {}", env!("CARGO_PKG_VERSION")),
            entries,
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading baseline {}", path.display()))?;
        let baseline: Baseline = serde_json::from_str(&raw)
            .with_context(|| format!("parsing baseline {}", path.display()))?;
        Ok(baseline)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let mut json = serde_json::to_string_pretty(self)?;
        json.push('\n');
        std::fs::write(path, json)
            .with_context(|| format!("writing baseline {}", path.display()))?;
        Ok(())
    }

    /// Drops findings this baseline already knows about.
    pub fn apply(&self, report: &mut Report, root: &Path) -> Applied {
        let recorded: HashSet<&Entry> = self.entries.iter().collect();

        let present: HashSet<Entry> = report
            .findings
            .iter()
            .map(|f| Entry {
                rule: f.rule.to_string(),
                file: relative_path(&f.file, root),
                test: f.test_name.clone(),
            })
            .collect();

        let before = report.findings.len();
        report.findings.retain(|f| {
            let key = Entry {
                rule: f.rule.to_string(),
                file: relative_path(&f.file, root),
                test: f.test_name.clone(),
            };
            !recorded.contains(&key)
        });

        let suppressed = before - report.findings.len();
        report.suppressed += suppressed;

        Applied {
            suppressed,
            stale: self
                .entries
                .iter()
                .filter(|entry| !present.contains(*entry))
                .count(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{Confidence, Finding};
    use std::path::PathBuf;

    fn finding(rule: &'static str, file: &str, test: &str, line: usize) -> Finding {
        Finding {
            rule,
            confidence: Confidence::Certain,
            file: PathBuf::from(file),
            line,
            test_name: test.to_string(),
            test_line: line,
            message: "…".to_string(),
        }
    }

    fn report_of(findings: Vec<Finding>) -> Report {
        Report {
            findings,
            tests_scanned: 100,
            files_scanned: 5,
            suppressed: 0,
        }
    }

    #[test]
    fn suppresses_only_recorded_findings() {
        let root = Path::new("");
        let recorded = report_of(vec![finding("no-assertions", "tests/a.py", "test_one", 10)]);
        let baseline = Baseline::from_report(&recorded, root);

        let mut current = report_of(vec![
            finding("no-assertions", "tests/a.py", "test_one", 10),
            finding("no-assertions", "tests/a.py", "test_two", 20),
        ]);
        let applied = baseline.apply(&mut current, root);

        assert_eq!(applied.suppressed, 1);
        assert_eq!(current.findings.len(), 1);
        assert_eq!(current.findings[0].test_name, "test_two");
        assert_eq!(current.suppressed, 1);
    }

    /// The whole point of keying on the test name: edits above a test shift its
    /// line number, and that must not resurrect a baselined finding.
    #[test]
    fn survives_the_test_moving_to_a_different_line() {
        let root = Path::new("");
        let baseline = Baseline::from_report(
            &report_of(vec![finding("no-assertions", "tests/a.py", "test_one", 10)]),
            root,
        );

        let mut moved = report_of(vec![finding(
            "no-assertions",
            "tests/a.py",
            "test_one",
            402,
        )]);
        let applied = baseline.apply(&mut moved, root);

        assert_eq!(applied.suppressed, 1);
        assert!(moved.findings.is_empty());
    }

    /// A different rule firing on the same test is a new finding.
    #[test]
    fn does_not_suppress_a_different_rule_on_the_same_test() {
        let root = Path::new("");
        let baseline = Baseline::from_report(
            &report_of(vec![finding("no-assertions", "tests/a.py", "test_one", 10)]),
            root,
        );

        let mut current = report_of(vec![finding(
            "swallowed-failure",
            "tests/a.py",
            "test_one",
            10,
        )]);
        baseline.apply(&mut current, root);

        assert_eq!(current.findings.len(), 1);
    }

    #[test]
    fn counts_entries_that_no_longer_exist() {
        let root = Path::new("");
        let baseline = Baseline::from_report(
            &report_of(vec![
                finding("no-assertions", "tests/a.py", "test_one", 10),
                finding("no-assertions", "tests/a.py", "test_gone", 30),
            ]),
            root,
        );

        let mut current = report_of(vec![finding("no-assertions", "tests/a.py", "test_one", 10)]);
        let applied = baseline.apply(&mut current, root);

        assert_eq!(applied.suppressed, 1);
        assert_eq!(applied.stale, 1);
    }

    #[test]
    fn round_trips_through_json() {
        let baseline = Baseline::from_report(
            &report_of(vec![finding("no-assertions", "tests/a.py", "test_one", 10)]),
            Path::new(""),
        );
        let json = serde_json::to_string(&baseline).unwrap();
        let parsed: Baseline = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.entries, baseline.entries);
    }
}
