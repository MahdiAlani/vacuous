//! pytest and unittest vocabulary.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use tree_sitter::Node;

use super::{LanguageAdapter, TestFn};
use crate::parse::{descendants, line_of, text};

pub struct Python;

/// Substrings hinting that a call checks something. Only used to suppress, so
/// being generous is the safe direction.
const VERIFICATION_HINTS: &[&str] = &[
    "assert", "check", "verify", "validate", "expect", "ensure", "compare", "match", "confirm",
];

/// `unittest.mock` assertions. These check recorded calls, so literal arguments
/// don't make them tautological the way `assertEqual(1, 1)` is.
const MOCK_ASSERTION_PREFIXES: &[&str] = &[
    "assert_called",
    "assert_not_called",
    "assert_any_call",
    "assert_has_calls",
    "assert_awaited",
    "assert_not_awaited",
];

/// The nose/SQLAlchemy assertion helpers. No `assert` prefix, but that's what
/// they are, and some suites use nothing else.
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
        if stem == "conftest" {
            return false;
        }
        // pytest/unittest discovery: test_*.py, tests.py, *_test.py.
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
            "assert_statement" => true,
            "call" => {
                let Some(name) = callee_name(node, src) else {
                    return false;
                };
                // Catches unittest, numpy/pandas `assert_allclose`, hamcrest
                // `assert_that`, and mock's `assert_called_*`.
                name.starts_with("assert")
                    || name == "raises"
                    || name == "fail"
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
        // name -> (asserts directly, names it calls)
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

            // Nested defs collide with the outer name; last one wins. Harmless,
            // since this only ever suppresses.
            defs.insert(name.to_string(), (asserts_directly, calls));
        }

        let mut asserting: HashSet<String> = defs
            .iter()
            .filter(|(_, (direct, _))| *direct)
            .map(|(name, _)| name.clone())
            .collect();

        // Propagate to a fixed point so a -> b -> assert counts.
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
        if node.kind() == "assert_statement" {
            // First named child is the condition. The optional message is
            // usually a literal and would skew this.
            let condition = node.named_child(0)?;
            return eval_const(condition, src).map(|value| value.truthy());
        }

        if node.kind() != "call" {
            return None;
        }
        let name = callee_name(node, src)?;
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

        // Only the forms whose semantics we model. Anything else stays `None`.
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

        // Decorators hang off the parent `decorated_definition`.
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
        // Re-raising, asserting, or failing means it isn't swallowing.
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
                    return true;
                };
                if decorator_may_assert(&path) {
                    return true;
                }
            }
        }

        // Same story a level down: an inner function carrying the decorator.
        descendants(test.body).any(|node| node.kind() == "decorated_definition")
    }

    fn body_returns_value(&self, body: Node<'_>) -> bool {
        // Direct children only. A `return` inside a nested helper says nothing
        // about what the test hands back.
        let mut cursor = body.walk();
        body.named_children(&mut cursor).any(|statement| {
            statement.kind() == "return_statement" && statement.named_child(0).is_some()
        })
    }

    fn is_empty_body(&self, body: Node<'_>, src: &str) -> bool {
        let mut cursor = body.walk();
        for stmt in body.named_children(&mut cursor) {
            match stmt.kind() {
                "pass_statement" => continue,
                "expression_statement" => {
                    let inner = text(stmt, src).trim();
                    let is_docstring = inner.starts_with('"') || inner.starts_with('\'');
                    if is_docstring || inner == "..." {
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

/// Module-level functions and class methods, yes. Anything nested inside another
/// function, no.
fn is_collectible(func: Node<'_>) -> bool {
    let mut parent = func.parent();
    while let Some(node) = parent {
        match node.kind() {
            "function_definition" => return false,
            "module" => return true,
            _ => parent = node.parent(),
        }
    }
    true
}

/// `np.testing.assert_allclose(...)` -> `assert_allclose`
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

/// Decorators known not to assert. Anything missing from here is treated as
/// capable of asserting, so add deliberately.
const INERT_DECORATORS: &[&str] = &[
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
    "patch",
    "object",
    "dict",
    "staticmethod",
    "classmethod",
    "property",
];

/// `pytest.mark.*` entries that actually do something. pytest-benchmark's
/// fixture measures and checks; the rest are labels.
const ACTIVE_PYTEST_MARKERS: &[&str] = &["benchmark"];

fn decorator_may_assert(path: &str) -> bool {
    let last = path.rsplit('.').next().unwrap_or(path);

    if INERT_DECORATORS.contains(&last) {
        return false;
    }
    if path.starts_with("pytest.mark.") || path.starts_with("mark.") {
        return ACTIVE_PYTEST_MARKERS.contains(&last);
    }
    true
}

/// `@pytest.mark.benchmark(group='x')` -> `pytest.mark.benchmark`
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

#[derive(Debug, Clone, PartialEq)]
enum Const {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    None,
}

impl Const {
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

/// Evaluates expressions built only from literals, and gives up as soon as
/// anything else appears. No arithmetic, no ordered comparisons: a gap here
/// costs a missed finding, which is cheaper than a wrong one.
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
        // Empty strings have no `string_content` child.
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

/// Two-operand equality and identity only. Chained and ordered comparisons give
/// up rather than risk being wrong.
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
    // Joined so `is not` arrives as one token.
    match operators.join(" ").as_str() {
        "==" | "is" => Some(Const::Bool(left == right)),
        "!=" | "is not" => Some(Const::Bool(left != right)),
        _ => None,
    }
}

/// Contents of a string literal. The grammar splits strings into start/content/
/// end, so this avoids stripping `r`, `f` and triple quotes by hand.
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
            // patch("a.b.c"), mock.patch(...), mocker.patch(...)
            "patch" => {
                if let Some(first) = arg_nodes.first()
                    && let Some(symbol) = last_dotted_segment(*first, src)
                {
                    out.push(symbol);
                }
            }
            // patch.object(SomeClass, "method")
            "object" => {
                if let Some(second) = arg_nodes.get(1)
                    && let Some(symbol) = string_literal_value(*second, src)
                {
                    out.push(symbol.to_string());
                }
            }
            // monkeypatch.setattr(mod, "name", repl) or setattr("mod.name", repl)
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

fn catches_assertion_errors(clause: Node<'_>, src: &str) -> bool {
    let mut cursor = clause.walk();
    for child in clause.named_children(&mut cursor) {
        // Comments are named nodes, so `except:  # noqa` would otherwise read as
        // an exception type.
        if matches!(child.kind(), "block" | "comment") {
            continue;
        }
        let rendered = text(child, src);
        return ASSERTION_CATCHING
            .iter()
            .any(|exception| rendered.contains(exception));
    }
    // Bare `except:` catches everything.
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
        assert!(!is_test("tests/test_auth.pyc"));
        assert!(!is_test("README.md"));
        // Helper modules beside tests aren't collected by pytest.
        assert!(!is_test("tests/helpers.py"));
        assert!(!is_test("tests/factories.py"));
        assert!(!is_test("tests/conftest.py"));
    }
}
