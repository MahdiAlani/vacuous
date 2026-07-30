//! Python adapter: pytest and unittest vocabulary.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use tree_sitter::Node;

use super::{LanguageAdapter, TestFn};
use crate::parse::{descendants, line_of, text};

pub struct Python;

/// Substrings that suggest a call delegates verification to a helper.
/// Used only to *suppress* findings, so a generous list is the safe choice.
const VERIFICATION_HINTS: &[&str] = &[
    "assert", "check", "verify", "validate", "expect", "ensure", "compare", "match", "confirm",
];

impl LanguageAdapter for Python {
    fn name(&self) -> &'static str {
        "python"
    }

    fn language(&self) -> tree_sitter::Language {
        tree_sitter_python::LANGUAGE.into()
    }

    fn is_test_file(&self, path: &Path) -> bool {
        if path.extension().and_then(|e| e.to_str()) != Some("py") {
            return false;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            return false;
        };
        // `conftest.py` holds fixtures, not tests, and pytest never collects
        // tests from it. Excluding it avoids a whole class of false positives.
        if stem == "conftest" {
            return false;
        }
        // Mirrors pytest/unittest discovery: `test_*.py`, `tests.py`, `*_test.py`.
        stem.starts_with("test") || stem.ends_with("_test")
    }

    fn test_functions<'t>(&self, root: Node<'t>, src: &str) -> Vec<TestFn<'t>> {
        let mut out = Vec::new();
        for node in descendants(root) {
            if node.kind() != "function_definition" {
                continue;
            }
            let Some(name_node) = node.child_by_field_name("name") else {
                continue;
            };
            let name = text(name_node, src);
            if !name.starts_with("test") {
                continue;
            }
            // A test runner only collects module-level functions and methods of
            // classes. Functions nested inside another function are never
            // collected — and test suites are full of them, because route
            // handlers and CLI commands get named things like `test` and
            // `testcmd`. Flagging those would be a pure false positive.
            if !is_collectible(node) {
                continue;
            }
            let Some(body) = node.child_by_field_name("body") else {
                continue;
            };
            out.push(TestFn {
                name: name.to_string(),
                node,
                body,
                line: line_of(node),
            });
        }
        out
    }

    fn is_assertion(&self, node: Node<'_>, src: &str) -> bool {
        match node.kind() {
            // Bare `assert x == y`
            "assert_statement" => true,
            "call" => {
                let Some(name) = callee_name(node, src) else {
                    return false;
                };
                // Covers unittest (`assertEqual`, `assertRaises`), numpy/pandas
                // (`assert_allclose`, `assert_frame_equal`), hamcrest
                // (`assert_that`), and mock (`assert_called_once_with`).
                name.starts_with("assert")
                    // `pytest.raises(...)` / `self.assertRaises(...)`
                    || name == "raises"
                    // `pytest.fail(...)` / `self.fail(...)`
                    || name == "fail"
            }
            _ => false,
        }
    }

    fn is_verification_helper(&self, node: Node<'_>, src: &str) -> bool {
        let Some(name) = self.called_name(node, src) else {
            return false;
        };
        let lowered = name.to_ascii_lowercase();
        VERIFICATION_HINTS.iter().any(|hint| lowered.contains(hint))
    }

    fn called_name<'a>(&self, node: Node<'_>, src: &'a str) -> Option<&'a str> {
        if node.kind() != "call" {
            return None;
        }
        callee_name(node, src)
    }

    fn asserting_helpers(&self, root: Node<'_>, src: &str) -> HashSet<String> {
        // Step 1: for every function in the file, record whether it asserts
        // directly and which local names it calls.
        let mut defs: HashMap<String, (bool, Vec<String>)> = HashMap::new();

        for node in descendants(root) {
            if node.kind() != "function_definition" {
                continue;
            }
            let Some(name) = node.child_by_field_name("name").map(|n| text(n, src)) else {
                continue;
            };
            let Some(body) = node.child_by_field_name("body") else {
                continue;
            };

            let mut asserts_directly = false;
            let mut calls = Vec::new();
            for inner in descendants(body) {
                if self.is_assertion(inner, src) {
                    asserts_directly = true;
                }
                if let Some(callee) = self.called_name(inner, src) {
                    calls.push(callee.to_string());
                }
            }

            // Nested definitions share the outer name in this map; last one
            // wins, which is harmless because we only ever use this to
            // *suppress* findings.
            defs.insert(name.to_string(), (asserts_directly, calls));
        }

        // Step 2: propagate to a fixed point so `a -> b -> assert` counts.
        let mut asserting: HashSet<String> = defs
            .iter()
            .filter(|(_, (direct, _))| *direct)
            .map(|(name, _)| name.clone())
            .collect();

        loop {
            let mut changed = false;
            for (name, (_, calls)) in &defs {
                if asserting.contains(name) {
                    continue;
                }
                if calls.iter().any(|callee| asserting.contains(callee)) {
                    asserting.insert(name.clone());
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        asserting
    }

    fn is_empty_body(&self, body: Node<'_>, src: &str) -> bool {
        let mut cursor = body.walk();
        for stmt in body.named_children(&mut cursor) {
            match stmt.kind() {
                // `pass`
                "pass_statement" => continue,
                "expression_statement" => {
                    // A docstring or a bare `...` is not a real statement.
                    let inner = text(stmt, src).trim();
                    let is_docstring = inner.starts_with('"') || inner.starts_with('\'');
                    let is_ellipsis = inner == "...";
                    if is_docstring || is_ellipsis {
                        continue;
                    }
                    return false;
                }
                _ => return false,
            }
        }
        true
    }
}

/// Would a test runner collect this function?
///
/// True for module-level functions and methods of classes. False for anything
/// nested inside another function, however deeply.
fn is_collectible(func: Node<'_>) -> bool {
    let mut parent = func.parent();
    while let Some(node) = parent {
        match node.kind() {
            // Nested inside another function — never collected.
            "function_definition" => return false,
            "module" => return true,
            // `block`, `class_definition`, `decorated_definition` — keep walking.
            _ => parent = node.parent(),
        }
    }
    true
}

/// The final segment of a call's callee.
///
/// `self.assertEqual(...)` -> `assertEqual`
/// `np.testing.assert_allclose(...)` -> `assert_allclose`
/// `foo(...)` -> `foo`
fn callee_name<'a>(call: Node<'_>, src: &'a str) -> Option<&'a str> {
    let function = call.child_by_field_name("function")?;
    match function.kind() {
        "identifier" => Some(text(function, src)),
        "attribute" => function
            .child_by_field_name("attribute")
            .map(|a| text(a, src)),
        _ => Some(text(function, src)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_test(p: &str) -> bool {
        Python.is_test_file(Path::new(p))
    }

    #[test]
    fn recognises_runner_collected_files() {
        assert!(is_test("tests/test_auth.py"));
        assert!(is_test("auth_test.py"));
        assert!(is_test("tests.py"));
    }

    #[test]
    fn ignores_non_test_files() {
        // Not Python at all.
        assert!(!is_test("tests/test_auth.pyc"));
        assert!(!is_test("README.md"));
        // Helper modules living beside tests are not collected by pytest, and
        // scanning them would produce findings for code nobody calls a test.
        assert!(!is_test("tests/helpers.py"));
        assert!(!is_test("tests/factories.py"));
        // conftest.py holds fixtures, never tests.
        assert!(!is_test("tests/conftest.py"));
    }
}
