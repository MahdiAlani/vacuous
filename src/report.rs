//! Findings, confidence levels, and how we render them.
//!
//! The most important type here is [`Confidence`]. A linter that makes even one
//! false accusation gets uninstalled and never reinstalled, so every rule must
//! declare how sure it is, and we only show `Certain` and `Likely` by default.

use std::path::PathBuf;

/// How sure we are that a finding is a genuine problem.
///
/// Ordered deliberately so `>=` comparisons work for filtering:
/// `Possible < Likely < Certain`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    /// A heuristic worth a look, but plausible reasons exist to write it this way.
    Possible,
    /// Almost certainly a problem; a small number of legitimate exceptions exist.
    Likely,
    /// A structural fact about the code, not a judgement call. Cannot be a false positive.
    Certain,
}

impl Confidence {
    pub fn label(self) -> &'static str {
        match self {
            Confidence::Certain => "certain",
            Confidence::Likely => "likely",
            Confidence::Possible => "possible",
        }
    }

    /// Parse a `--min-confidence` value.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "certain" => Some(Confidence::Certain),
            "likely" => Some(Confidence::Likely),
            "possible" => Some(Confidence::Possible),
            _ => None,
        }
    }

    fn style(self) -> anstyle::Style {
        use anstyle::AnsiColor;
        match self {
            Confidence::Certain => anstyle::Style::new().fg_color(Some(AnsiColor::Red.into())),
            Confidence::Likely => anstyle::Style::new().fg_color(Some(AnsiColor::Yellow.into())),
            Confidence::Possible => {
                anstyle::Style::new().fg_color(Some(AnsiColor::BrightBlack.into()))
            }
        }
    }
}

/// One vacuous test, located and explained.
#[derive(Debug, Clone)]
pub struct Finding {
    /// Stable rule id, e.g. `no-assertions`. Used for config and suppression.
    pub rule: &'static str,
    pub confidence: Confidence,
    pub file: PathBuf,
    /// 1-based line of the offending test's `def`.
    pub line: usize,
    pub test_name: String,
    /// Why this is a problem, in plain language, specific to this occurrence.
    pub message: String,
}

/// The result of a whole scan.
#[derive(Debug, Default)]
pub struct Report {
    pub findings: Vec<Finding>,
    pub tests_scanned: usize,
    pub files_scanned: usize,
}

impl Report {
    /// Merge another report in. Used to fold per-file results after parallel scanning.
    pub fn absorb(&mut self, other: Report) {
        self.findings.extend(other.findings);
        self.tests_scanned += other.tests_scanned;
        self.files_scanned += other.files_scanned;
    }

    /// Sort findings into a stable, human-friendly order so output is
    /// deterministic regardless of the order files finished parsing.
    pub fn sort(&mut self) {
        self.findings.sort_by(|a, b| {
            a.file
                .cmp(&b.file)
                .then(a.line.cmp(&b.line))
                .then(a.rule.cmp(b.rule))
        });
    }

    pub fn retain_at_least(&mut self, min: Confidence) {
        self.findings.retain(|f| f.confidence >= min);
    }
}

/// Render a report for a terminal.
///
/// Uses `anstream` so colour is stripped automatically when piped and
/// translated correctly on legacy Windows consoles.
pub fn print_pretty(report: &Report, root: &std::path::Path) {
    use anstream::println;

    let bold = anstyle::Style::new().bold();
    let dim = anstyle::Style::new().fg_color(Some(anstyle::AnsiColor::BrightBlack.into()));

    println!();

    if report.findings.is_empty() {
        let green = anstyle::Style::new().fg_color(Some(anstyle::AnsiColor::Green.into()));
        println!(
            "  {green}No vacuous tests found{green:#} in {} tests across {} files.",
            report.tests_scanned, report.files_scanned
        );
        println!();
        return;
    }

    let pct = if report.tests_scanned == 0 {
        0.0
    } else {
        100.0 * report.findings.len() as f64 / report.tests_scanned as f64
    };

    println!(
        "  {bold}{} vacuous {}{bold:#} in {} ({:.1}%)",
        report.findings.len(),
        plural(report.findings.len(), "test", "tests"),
        thousands(report.tests_scanned),
        pct
    );
    println!();

    // Align the location column so the rule names line up and scan easily.
    let locations: Vec<String> = report
        .findings
        .iter()
        .map(|f| format!("{}:{}", display_path(&f.file, root), f.line))
        .collect();
    let loc_width = locations.iter().map(|l| l.len()).max().unwrap_or(0);
    let rule_width = report
        .findings
        .iter()
        .map(|f| f.rule.len())
        .max()
        .unwrap_or(0);

    for (f, loc) in report.findings.iter().zip(&locations) {
        let cs = f.confidence.style();
        println!(
            "  {loc:<loc_width$}  {:<rule_width$}  {cs}{}{cs:#}",
            f.rule,
            f.confidence.label(),
        );
        println!("  {dim}{:loc_width$}  └─ {}{dim:#}", "", f.message);
    }

    println!();
    println!(
        "  {dim}{} {} scanned{dim:#}",
        thousands(report.files_scanned),
        plural(report.files_scanned, "file", "files")
    );
    println!();
}

/// How to show a finding's path.
///
/// When scanning a directory we show the path relative to it. When scanning a
/// single file, `strip_prefix` would leave an empty string, so fall back to the
/// file name.
fn display_path<'a>(
    file: &'a std::path::Path,
    root: &std::path::Path,
) -> std::borrow::Cow<'a, str> {
    let relative = file.strip_prefix(root).unwrap_or(file);
    let shown = if relative.as_os_str().is_empty() {
        std::path::Path::new(file.file_name().unwrap_or_default())
    } else {
        relative
    };
    shown.to_string_lossy()
}

fn plural(n: usize, one: &'static str, many: &'static str) -> &'static str {
    if n == 1 { one } else { many }
}

/// `1240` -> `1,240`. Test counts get large enough that this matters for
/// readability, and the headline number is the whole point of the summary.
fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thousands_separates_correctly() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(7), "7");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000), "1,000");
        assert_eq!(thousands(1_240), "1,240");
        assert_eq!(thousands(12_345), "12,345");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }

    #[test]
    fn single_file_scan_shows_the_file_name() {
        let file = std::path::Path::new("tests/fixtures/should_flag.py");
        // root == the file itself, as happens with `vacuous check some_file.py`
        assert_eq!(display_path(file, file), "should_flag.py");
    }

    #[test]
    fn directory_scan_shows_a_relative_path() {
        let file = std::path::Path::new("repo/tests/test_auth.py");
        let root = std::path::Path::new("repo");
        assert_eq!(display_path(file, root), "tests/test_auth.py");
    }

    #[test]
    fn confidence_orders_from_possible_to_certain() {
        assert!(Confidence::Certain > Confidence::Likely);
        assert!(Confidence::Likely > Confidence::Possible);
    }
}
