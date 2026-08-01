//! Machine-readable output.
//!
//! JSON for scripting, SARIF for GitHub code scanning, which turns findings
//! into inline PR annotations.

use std::path::Path;

use serde_json::{Value, json};

use crate::report::{Confidence, Report, relative_path};

const TOOL_URL: &str = "https://github.com/MahdiAlani/vacuous";

pub fn to_json(report: &Report, root: &Path) -> String {
    let findings: Vec<Value> = report
        .findings
        .iter()
        .map(|f| {
            json!({
                "rule": f.rule,
                "confidence": f.confidence.label(),
                "file": relative_path(&f.file, root),
                "line": f.line,
                "test": f.test_name,
                "message": f.message,
            })
        })
        .collect();

    let document = json!({
        "tool": "vacuous",
        "version": env!("CARGO_PKG_VERSION"),
        "summary": {
            "findings": report.findings.len(),
            // Lower than `findings` when one test yields several.
            "tests_flagged": report.tests_flagged(),
            "suppressed": report.suppressed,
            "tests_scanned": report.tests_scanned,
            "files_scanned": report.files_scanned,
        },
        "findings": findings,
    });

    serde_json::to_string_pretty(&document).expect("report is always serialisable")
}

/// SARIF 2.1.0. Levels map straight from confidence: a structural fact is an
/// error, a judgement call is a warning, a hint is a note. GitHub doesn't block
/// merges on any of these unless you ask it to.
pub fn to_sarif(report: &Report, root: &Path) -> String {
    let rules: Vec<Value> = crate::rules::all_rules()
        .iter()
        .map(|rule| {
            json!({
                "id": rule.name(),
                "name": rule.name(),
                "shortDescription": { "text": rule.description() },
                "helpUri": TOOL_URL,
            })
        })
        .collect();

    let results: Vec<Value> = report
        .findings
        .iter()
        .map(|f| {
            let file = relative_path(&f.file, root);
            json!({
                "ruleId": f.rule,
                "level": match f.confidence {
                    Confidence::Certain => "error",
                    Confidence::Likely => "warning",
                    Confidence::Possible => "note",
                },
                "message": { "text": f.message },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": { "uri": file },
                        "region": { "startLine": f.line },
                    }
                }],
                // Deliberately excludes the line number, so an alert survives
                // unrelated edits above it rather than being reported as new.
                "partialFingerprints": {
                    "vacuousFinding/v1": format!(
                        "{}:{}:{}",
                        f.rule,
                        relative_path(&f.file, root),
                        f.test_name
                    ),
                },
            })
        })
        .collect();

    let document = json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "vacuous",
                    "version": env!("CARGO_PKG_VERSION"),
                    "informationUri": TOOL_URL,
                    "rules": rules,
                }
            },
            "results": results,
        }],
    });

    serde_json::to_string_pretty(&document).expect("report is always serialisable")
}
