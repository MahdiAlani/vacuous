//! Findings and how they're rendered.

use std::path::PathBuf;

/// Variants are ordered so `>=` works for filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    /// Worth a look, but there are plausible reasons to write it this way.
    Possible,
    /// Almost certainly a problem, with a few legitimate exceptions.
    Likely,
    /// A structural fact about the code rather than a judgement.
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

#[derive(Debug, Clone)]
pub struct Finding {
    /// Stable id, e.g. `no-assertions`, for config and suppression.
    pub rule: &'static str,
    pub confidence: Confidence,
    pub file: PathBuf,
    /// 1-based, pointing at whatever the check considers the offender.
    pub line: usize,
    pub test_name: String,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct Report {
    pub findings: Vec<Finding>,
    pub tests_scanned: usize,
    pub files_scanned: usize,
    /// Hidden by a baseline. Shown in the summary so they aren't invisible.
    pub suppressed: usize,
}

impl Report {
    /// Folds in a per-file result after parallel scanning.
    pub fn absorb(&mut self, other: Report) {
        self.findings.extend(other.findings);
        self.tests_scanned += other.tests_scanned;
        self.files_scanned += other.files_scanned;
        self.suppressed += other.suppressed;
    }

    /// Files finish parsing in arbitrary order, so sort before printing.
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

/// `anstream` strips colour when piped and handles older Windows consoles.
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
        if report.suppressed > 0 {
            println!(
                "  {dim}{} existing {} hidden by the baseline{dim:#}",
                thousands(report.suppressed),
                plural(report.suppressed, "finding", "findings")
            );
        }
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

    // Pad the location column so rule names line up.
    let locations: Vec<String> = report
        .findings
        .iter()
        .map(|f| format!("{}:{}", relative_path(&f.file, root), f.line))
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
    if report.suppressed > 0 {
        println!(
            "  {dim}{} existing {} hidden by the baseline{dim:#}",
            thousands(report.suppressed),
            plural(report.suppressed, "finding", "findings")
        );
    }
    println!();
}

/// Path relative to the scan root, always with forward slashes.
///
/// Normalising matters beyond looks: SARIF requires forward slashes, and a
/// baseline written on Windows has to match one read on Linux.
///
/// When the root *is* the file, `strip_prefix` leaves nothing, so fall back to
/// the file name.
pub fn relative_path(file: &std::path::Path, root: &std::path::Path) -> String {
    let relative = file.strip_prefix(root).unwrap_or(file);
    let shown = if relative.as_os_str().is_empty() {
        std::path::Path::new(file.file_name().unwrap_or_default())
    } else {
        relative
    };
    shown.to_string_lossy().replace('\\', "/")
}

fn plural(n: usize, one: &'static str, many: &'static str) -> &'static str {
    if n == 1 { one } else { many }
}

/// `1240` -> `1,240`. Test counts get big enough to need it.
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
        assert_eq!(relative_path(file, file), "should_flag.py");
    }

    #[test]
    fn directory_scan_shows_a_relative_path() {
        let file = std::path::Path::new("repo/tests/test_auth.py");
        let root = std::path::Path::new("repo");
        assert_eq!(relative_path(file, root), "tests/test_auth.py");
    }

    /// SARIF requires forward slashes, and baselines have to survive being
    /// written on one platform and read on another. Built from components so
    /// the separator is whatever this platform uses.
    #[test]
    fn paths_are_normalised_to_forward_slashes() {
        let file: std::path::PathBuf = ["repo", "tests", "test_auth.py"].iter().collect();
        let root = std::path::Path::new("repo");
        assert_eq!(relative_path(&file, root), "tests/test_auth.py");
    }

    #[test]
    fn confidence_orders_from_possible_to_certain() {
        assert!(Confidence::Certain > Confidence::Likely);
        assert!(Confidence::Likely > Confidence::Possible);
    }
}
