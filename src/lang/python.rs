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

/// `unittest.mock` assertions. These check recorded calls, so literal arguments
/// do not make them tautological the way `assertEqual(1, 1)` is.
const MOCK_ASSERTION_PREFIXES: &[&str] = &[
    "assert_called",
    "assert_not_called",
    "assert_any_call",
    "assert_has_calls",
    "assert_awaited",
    "assert_not_awaited",
];

/// Assertion functions from widely used test-support conventions that carry no
/// `assert` prefix.
///
/// The nose/SQLAlchemy family. SQLAlchemy's suite is built almost entirely on
/// these — without them we reported 36% of its 12,716 tests as vacuous, which
/// was nonsense.
const KNOWN_ASSERTION_FUNCTIONS: &[&str] = &[
    "eq_",
    "ne_",
    "is_",
    "is_not",
    "is_true",
    "is_false",
    "is_none",
    "is_not_none",
    "is_instance_of",
    "in_",
    "not_in",
    "ok_",
    "eq_regex",
    "eq_ignore_whitespace",
    "expect",
];

/// Exception types that catch a failed assertion.
const ASSERTION_CATCHING: &[&str] = &["Exception", "BaseException", "AssertionError"];

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
                    // `eq_(a, b)` and friends.
                    || KNOWN_ASSERTION_FUNCTIONS.contains(&name)
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

    fn constant_assertion_outcome(&self, node: Node<'_>, src: &str) -> Option<bool> {
        // Bare `assert <condition>[, <message>]`. Only the condition counts —
        // the message is nearly always a literal and would poison the analysis.
        if node.kind() == "assert_statement" {
            let condition = node.named_child(0)?;
            return eval_const(condition, src).map(|value| value.truthy());
        }

        if node.kind() != "call" {
            return None;
        }
        let name = callee_name(node, src)?;
        // Mock-history assertions check recorded behaviour, so literal
        // arguments do not make them tautological.
        if MOCK_ASSERTION_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            return None;
        }

        let arguments = node.child_by_field_name("arguments")?;
        let mut cursor = arguments.walk();
        let args: Vec<Node> = arguments
            .named_children(&mut cursor)
            .filter(|n| n.kind() != "comment")
            .collect();

        // Only assertions whose semantics we actually model. Anything else with
        // constant arguments is left alone rather than guessed at.
        match name {
            "assertTrue" => eval_const(*args.first()?, src).map(|v| v.truthy()),
            "assertFalse" => eval_const(*args.first()?, src).map(|v| !v.truthy()),
            "assertEqual" | "assertEquals" | "assertIs" => {
                let left = eval_const(*args.first()?, src)?;
                let right = eval_const(*args.get(1)?, src)?;
                Some(left == right)
            }
            "assertNotEqual" | "assertIsNot" => {
                let left = eval_const(*args.first()?, src)?;
                let right = eval_const(*args.get(1)?, src)?;
                Some(left != right)
            }
            "assertIsNone" => eval_const(*args.first()?, src).map(|v| v == Const::None),
            "assertIsNotNone" => eval_const(*args.first()?, src).map(|v| v != Const::None),
            _ => None,
        }
    }

    fn is_mock_assertion(&self, node: Node<'_>, src: &str) -> bool {
        let Some(name) = self.called_name(node, src) else {
            return false;
        };
        MOCK_ASSERTION_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
    }

    fn patched_symbols(&self, test: &TestFn<'_>, src: &str) -> Vec<String> {
        let mut out = Vec::new();

        // Decorators hang off the parent `decorated_definition`, not the
        // function node itself.
        if let Some(parent) = test.node.parent()
            && parent.kind() == "decorated_definition"
        {
            let mut cursor = parent.walk();
            for child in parent.named_children(&mut cursor) {
                if child.kind() == "decorator" {
                    collect_patch_targets(child, src, &mut out);
                }
            }
        }

        // `mocker.patch(...)`, `with patch(...)`, `monkeypatch.setattr(...)`.
        collect_patch_targets(test.body, src, &mut out);

        out
    }

    fn implied_subject(&self, test_name: &str) -> Option<String> {
        let stripped = test_name
            .strip_prefix("test_")
            .or_else(|| test_name.strip_prefix("test"))?;
        let subject = stripped.trim_start_matches('_');
        if subject.is_empty() {
            None
        } else {
            Some(subject.to_string())
        }
    }

    fn is_swallowing_handler(&self, node: Node<'_>, src: &str) -> bool {
        if node.kind() != "except_clause" {
            return false;
        }
        if !catches_assertion_errors(node, src) {
            return false;
        }
        // A handler that re-raises, asserts, or fails does not swallow.
        for inner in descendants(node) {
            if inner.kind() == "raise_statement" || self.is_assertion(inner, src) {
                return false;
            }
        }
        true
    }

    fn is_terminating_statement(&self, node: Node<'_>) -> bool {
        matches!(node.kind(), "return_statement" | "raise_statement")
    }

    fn assertions_may_come_from_decorator(&self, test: &TestFn<'_>, src: &str) -> bool {
        if let Some(parent) = test.node.parent()
            && parent.kind() == "decorated_definition"
        {
            let mut cursor = parent.walk();
            for child in parent.named_children(&mut cursor) {
                if child.kind() != "decorator" {
                    continue;
                }
                let Some(path) = decorator_path(child, src) else {
                    // Could not read it, so we cannot rule it out.
                    return true;
                };
                if decorator_may_assert(&path) {
                    return true;
                }
            }
        }

        // A decorated *nested* function is the same story one level down:
        // SQLAlchemy's `test_update_whereclause` defines an inner `go()` carrying
        // `@profiling.function_call_count()` and calls it.
        descendants(test.body).any(|node| node.kind() == "decorated_definition")
    }

    fn body_returns_value(&self, body: Node<'_>) -> bool {
        // Direct children only: a `return` inside a nested helper function says
        // nothing about what the test itself hands back.
        let mut cursor = body.walk();
        body.named_children(&mut cursor).any(|statement| {
            statement.kind() == "return_statement" && statement.named_child(0).is_some()
        })
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

/// Decorators known not to assert anything. Anything absent from this list is
/// assumed capable of asserting, so additions here should be deliberate.
const INERT_DECORATORS: &[&str] = &[
    // pytest markers and fixtures
    "parametrize",
    "skip",
    "skipif",
    "xfail",
    "usefixtures",
    "filterwarnings",
    "fixture",
    "timeout",
    "asyncio",
    "anyio",
    "flaky",
    "repeat",
    "django_db",
    "freeze_time",
    // mock
    "patch",
    "object",
    "dict",
    // builtins
    "staticmethod",
    "classmethod",
    "property",
];

/// `pytest.mark.*` entries that *do* carry behaviour, contrary to the usual rule
/// that a marker is inert. `benchmark` belongs to pytest-benchmark, where the
/// fixture performs the measurement and the checking.
const ACTIVE_PYTEST_MARKERS: &[&str] = &["benchmark"];

/// Could this decorator assert on the test's behalf?
fn decorator_may_assert(path: &str) -> bool {
    let last = path.rsplit('.').next().unwrap_or(path);

    if INERT_DECORATORS.contains(&last) {
        return false;
    }
    // A plain `pytest.mark.something` is just a label unless a plugin gives it
    // behaviour.
    if path.starts_with("pytest.mark.") || path.starts_with("mark.") {
        return ACTIVE_PYTEST_MARKERS.contains(&last);
    }
    true
}

/// The dotted name a decorator refers to, without its arguments.
///
/// `@pytest.mark.benchmark(group='x')` -> `pytest.mark.benchmark`
/// `@flaky`                            -> `flaky`
fn decorator_path(decorator: Node<'_>, src: &str) -> Option<String> {
    let mut cursor = decorator.walk();
    let expression = decorator
        .named_children(&mut cursor)
        .find(|child| child.kind() != "comment")?;
    let target = if expression.kind() == "call" {
        expression.child_by_field_name("function")?
    } else {
        expression
    };
    Some(text(target, src).to_string())
}

/// A value we could work out without running anything.
#[derive(Debug, Clone, PartialEq)]
enum Const {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    None,
}

impl Const {
    /// Python's truthiness rules.
    fn truthy(&self) -> bool {
        match self {
            Const::Bool(b) => *b,
            Const::Int(i) => *i != 0,
            Const::Float(f) => *f != 0.0,
            Const::Str(s) => !s.is_empty(),
            Const::None => false,
        }
    }
}

/// Evaluate an expression that depends on nothing but literals.
///
/// Returns `None` the moment anything from the code under test appears, which is
/// the common case and the safe default. Deliberately narrow: no arithmetic, no
/// ordered comparisons. Every gap here is a missed finding, never a false one.
fn eval_const(node: Node<'_>, src: &str) -> Option<Const> {
    match node.kind() {
        "true" => Some(Const::Bool(true)),
        "false" => Some(Const::Bool(false)),
        "none" => Some(Const::None),
        "integer" => text(node, src)
            .replace('_', "")
            .parse()
            .ok()
            .map(Const::Int),
        "float" => text(node, src)
            .replace('_', "")
            .parse()
            .ok()
            .map(Const::Float),
        // An empty string has no `string_content` child, hence the fallback.
        "string" => Some(Const::Str(
            string_literal_value(node, src).unwrap_or("").to_string(),
        )),
        "parenthesized_expression" => node.named_child(0).and_then(|inner| eval_const(inner, src)),
        "not_operator" => {
            let argument = node.child_by_field_name("argument")?;
            Some(Const::Bool(!eval_const(argument, src)?.truthy()))
        }
        "boolean_operator" => {
            let left = eval_const(node.child_by_field_name("left")?, src)?;
            let right = eval_const(node.child_by_field_name("right")?, src)?;
            let operator = node.child_by_field_name("operator")?;
            match text(operator, src) {
                "and" => Some(Const::Bool(left.truthy() && right.truthy())),
                "or" => Some(Const::Bool(left.truthy() || right.truthy())),
                _ => None,
            }
        }
        "comparison_operator" => eval_comparison(node, src),
        _ => None,
    }
}

/// Only the two-operand equality and identity forms. Chained comparisons and
/// ordered ones (`<`, `>=`) return `None` rather than risk being wrong.
fn eval_comparison(node: Node<'_>, src: &str) -> Option<Const> {
    let mut cursor = node.walk();
    let mut operands = Vec::new();
    let mut operators = Vec::new();
    for child in node.children(&mut cursor) {
        if child.kind() == "comment" {
            continue;
        }
        if child.is_named() {
            operands.push(child);
        } else {
            operators.push(text(child, src));
        }
    }
    if operands.len() != 2 {
        return None;
    }

    let left = eval_const(operands[0], src)?;
    let right = eval_const(operands[1], src)?;
    // Joined so that `is not` and `not in` arrive as one token.
    match operators.join(" ").as_str() {
        "==" | "is" => Some(Const::Bool(left == right)),
        "!=" | "is not" => Some(Const::Bool(left != right)),
        _ => None,
    }
}

/// The text inside a string literal, without quotes or prefixes.
///
/// The grammar splits a string into `string_start` / `string_content` /
/// `string_end`, so we can ask for the content directly rather than trying to
/// strip `r`, `f`, and triple quotes by hand.
fn string_literal_value<'a>(node: Node<'_>, src: &'a str) -> Option<&'a str> {
    if node.kind() != "string" {
        return None;
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| child.kind() == "string_content")
        .map(|content| text(content, src))
}

/// `"myapp.services.charge_card"` -> `charge_card`
fn last_dotted_segment(node: Node<'_>, src: &str) -> Option<String> {
    let value = string_literal_value(node, src)?;
    value.rsplit('.').next().map(|s| s.to_string())
}

/// Collect every symbol replaced by a mock within `root`.
fn collect_patch_targets(root: Node<'_>, src: &str, out: &mut Vec<String>) {
    for node in descendants(root) {
        if node.kind() != "call" {
            continue;
        }
        let Some(name) = callee_name(node, src) else {
            continue;
        };
        let Some(args) = node.child_by_field_name("arguments") else {
            continue;
        };
        let mut cursor = args.walk();
        let arg_nodes: Vec<Node> = args
            .named_children(&mut cursor)
            .filter(|n| n.kind() != "comment")
            .collect();

        match name {
            // patch("a.b.c"), mock.patch("a.b.c"), mocker.patch("a.b.c")
            "patch" => {
                if let Some(first) = arg_nodes.first()
                    && let Some(symbol) = last_dotted_segment(*first, src)
                {
                    out.push(symbol);
                }
            }
            // patch.object(SomeClass, "method") — `callee_name` yields "object".
            "object" => {
                if let Some(second) = arg_nodes.get(1)
                    && let Some(symbol) = string_literal_value(*second, src)
                {
                    out.push(symbol.to_string());
                }
            }
            // monkeypatch.setattr(module, "name", replacement)
            // monkeypatch.setattr("module.name", replacement)
            "setattr" => {
                let from_second = arg_nodes.get(1).and_then(|n| string_literal_value(*n, src));
                if let Some(symbol) = from_second {
                    out.push(symbol.to_string());
                } else if let Some(first) = arg_nodes.first()
                    && let Some(symbol) = last_dotted_segment(*first, src)
                {
                    out.push(symbol);
                }
            }
            _ => {}
        }
    }
}

/// Would this handler catch a failed assertion?
///
/// `except ValueError:` would not — an `AssertionError` escapes it — so only
/// bare handlers and those naming `Exception`/`BaseException`/`AssertionError`
/// count.
fn catches_assertion_errors(clause: Node<'_>, src: &str) -> bool {
    let mut cursor = clause.walk();
    for child in clause.named_children(&mut cursor) {
        // Comments are named nodes, so `except:  # noqa` would otherwise look
        // like it names an exception type.
        if matches!(child.kind(), "block" | "comment") {
            continue;
        }
        // The first remaining child is the exception expression.
        let rendered = text(child, src);
        return ASSERTION_CATCHING
            .iter()
            .any(|exception| rendered.contains(exception));
    }
    // A bare `except:` catches everything, including assertion failures.
    true
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
