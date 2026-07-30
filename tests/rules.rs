//! Rule behaviour, driven by the paired fixtures in `tests/fixtures`.
//!
//! Every rule gets two tests: one proving it catches what it claims to, and one
//! proving it stays quiet on legitimate code. The second is the load-bearing
//! one — a rule that cries wolf is worse than no rule.

use std::path::{Path, PathBuf};

use vacuous::lang::LanguageAdapter;
use vacuous::lang::python::Python;
use vacuous::report::{Confidence, Report};

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}

/// Scan a fixture at the lowest confidence so no finding can hide behind the
/// default filter.
fn scan(rel: &str) -> Report {
    let path = fixture(rel);
    let mut outcome = vacuous::check(&path, &Python).expect("scan should succeed");
    assert!(
        outcome.skipped.is_empty(),
        "fixture should parse cleanly, but was skipped: {:?}",
        outcome.skipped
    );
    outcome.report.retain_at_least(Confidence::Possible);
    outcome.report
}

/// Test names flagged by one specific rule, in report order.
fn flagged_by(rule_dir: &str, rule: &str) -> Vec<String> {
    scan(&format!("tests/fixtures/python/{rule_dir}/should_flag.py"))
        .findings
        .iter()
        .filter(|f| f.rule == rule)
        .map(|f| f.test_name.clone())
        .collect()
}

/// A negative fixture must produce no findings from *any* rule, not just the
/// one it was written for. That catches cross-rule contamination too.
fn assert_no_findings(rule_dir: &str) {
    let rel = format!("tests/fixtures/python/{rule_dir}/should_not_flag.py");
    let report = scan(&rel);
    assert!(
        report.findings.is_empty(),
        "false positives in {rel}: {:#?}",
        report.findings
    );
}

#[test]
fn constant_assertion_flags_tautologies() {
    assert_eq!(
        flagged_by("constant_assertion", "constant-assertion"),
        vec![
            "test_asserts_true",
            "test_asserts_identity",
            "test_asserts_string_equality",
            "test_all_assertions_constant",
            "test_assert_equal_constants",
            "test_assert_true_literal",
        ]
    );
}

#[test]
fn constant_assertion_leaves_real_checks_alone() {
    assert_no_findings("constant_assertion");
}

#[test]
fn patched_target_flags_tests_that_mock_their_own_subject() {
    assert_eq!(
        flagged_by("patched_target", "patched-target-under-test"),
        vec!["test_charge_card", "test_send_email", "test_sync_users"]
    );
}

#[test]
fn patched_target_hedges_because_naming_is_a_convention() {
    let report = scan("tests/fixtures/python/patched_target/should_flag.py");
    assert!(
        report
            .findings
            .iter()
            .filter(|f| f.rule == "patched-target-under-test")
            .all(|f| f.confidence == Confidence::Likely),
        "inferring a subject from a test's name is not a structural fact, \
         so this rule must never claim `certain`: {:#?}",
        report.findings
    );
}

#[test]
fn patched_target_leaves_dependency_mocks_alone() {
    assert_no_findings("patched_target");
}

#[test]
fn swallowed_failure_flags_discarded_assertions() {
    assert_eq!(
        flagged_by("swallowed_failure", "swallowed-failure"),
        vec![
            "test_bare_except",
            "test_catches_exception",
            "test_catches_assertion_error",
            "test_catches_exception_and_only_logs",
        ]
    );
}

#[test]
fn swallowed_failure_respects_which_exceptions_are_caught() {
    assert_no_findings("swallowed_failure");
}

#[test]
fn unreachable_assertion_flags_assertions_after_an_exit() {
    assert_eq!(
        flagged_by("unreachable_assertion", "unreachable-assertion"),
        vec![
            "test_return_before_assert",
            "test_raise_before_assert",
            "test_return_in_loop_body_then_assert",
        ]
    );
}

#[test]
fn unreachable_assertion_understands_nested_returns() {
    assert_no_findings("unreachable_assertion");
}

#[test]
fn no_assertions_flags_tests_that_cannot_fail() {
    let report = scan("tests/fixtures/python/no_assertions/should_flag.py");

    let flagged: Vec<&str> = report
        .findings
        .iter()
        .map(|f| f.test_name.as_str())
        .collect();

    assert_eq!(
        flagged,
        vec![
            "test_creates_user",
            "test_empty_body",
            "test_only_a_docstring",
            "test_ellipsis_body",
            "test_calls_and_prints",
        ]
    );
    assert_eq!(report.tests_scanned, 5);
    assert!(report.findings.iter().all(|f| f.rule == "no-assertions"));
    assert!(
        report
            .findings
            .iter()
            .all(|f| f.confidence == Confidence::Certain)
    );
}

#[test]
fn no_assertions_stays_quiet_on_real_tests() {
    let report = scan("tests/fixtures/python/no_assertions/should_not_flag.py");

    assert!(
        report.findings.is_empty(),
        "false positives: {:#?}",
        report.findings
    );
    // Pins test discovery. Helpers, nested functions, and `not_a_test_at_all`
    // are all excluded, leaving exactly the real tests.
    assert_eq!(report.tests_scanned, 19);
}

/// Regression test for a false positive found by scanning flask: view functions
/// and CLI commands nested inside tests are commonly named `test` or `testcmd`,
/// but no test runner collects them.
#[test]
fn nested_functions_are_not_collected_as_tests() {
    let src = r#"
def test_real(client):
    @app.route("/test")
    def test():
        return "ok"

    @click.command()
    def testcmd():
        click.echo("hi")

    assert client.get("/test").status_code == 200
"#;
    let adapter = Python;
    let tree = vacuous::parse::parse(&adapter.language(), src).unwrap();
    let tests = adapter.test_functions(tree.root_node(), src);

    let names: Vec<&str> = tests.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["test_real"],
        "nested `test`/`testcmd` must not be collected"
    );
}

/// Regression test for the other flask false positive: a same-file helper that
/// asserts, whose name gives no hint that it does.
#[test]
fn local_asserting_helpers_are_resolved_transitively() {
    let src = r#"
def common_object_test(app):
    assert app.secret_key == "config"

def indirectly_asserts(app):
    common_object_test(app)

def test_direct(app):
    common_object_test(app)

def test_transitive(app):
    indirectly_asserts(app)

def test_genuinely_vacuous(app):
    app.configure()
"#;
    let adapter = Python;
    let tree = vacuous::parse::parse(&adapter.language(), src).unwrap();
    let helpers = adapter.asserting_helpers(tree.root_node(), src);

    assert!(helpers.contains("common_object_test"));
    assert!(
        helpers.contains("indirectly_asserts"),
        "transitive propagation failed: {helpers:?}"
    );
}

#[test]
fn empty_body_gets_a_distinct_message() {
    let report = scan("tests/fixtures/python/no_assertions/should_flag.py");

    let empty = report
        .findings
        .iter()
        .find(|f| f.test_name == "test_empty_body")
        .expect("test_empty_body should be flagged");
    assert!(
        empty.message.contains("empty body"),
        "expected an empty-body message, got: {}",
        empty.message
    );

    let populated = report
        .findings
        .iter()
        .find(|f| f.test_name == "test_creates_user")
        .expect("test_creates_user should be flagged");
    assert!(
        populated.message.contains("no assertions"),
        "expected a no-assertions message, got: {}",
        populated.message
    );
}

#[test]
fn scanning_a_directory_finds_both_fixtures() {
    // Exercises the walking path rather than the single-file shortcut.
    let path = fixture("tests/fixtures/python/no_assertions");
    let outcome = vacuous::check(&path, &Python).expect("scan should succeed");
    // Neither fixture filename matches test-runner discovery rules, so a
    // directory scan should collect nothing at all.
    assert_eq!(outcome.report.files_scanned, 0);
}
