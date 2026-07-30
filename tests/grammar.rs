//! Grammar smoke test.
//!
//! tree-sitter grammar crates version independently of the core crate (we pair
//! core 0.26 with the Python grammar 0.25), and they interoperate through
//! `tree-sitter-language`. That contract holds today but is not guaranteed
//! forever, so this test fails loudly in CI the moment an ABI mismatch appears,
//! rather than letting it surface as mysteriously empty scan results.

use vacuous::lang::LanguageAdapter;
use vacuous::lang::python::Python;
use vacuous::parse;

const SAMPLE: &str = r#"
import pytest


class TestThing:
    @pytest.mark.parametrize("value", [1, 2])
    async def test_async_method(self, value):
        with pytest.raises(ValueError):
            await parse(value)


def test_walrus_and_fstrings():
    if (n := compute()) > 0:
        assert f"{n}" == "1"
"#;

#[test]
fn python_grammar_parses_modern_syntax_without_errors() {
    let adapter = Python;
    let tree = parse::parse(&adapter.language(), SAMPLE).expect("parser should be constructible");

    assert!(
        !tree.root_node().has_error(),
        "Python grammar produced error nodes — likely a tree-sitter ABI mismatch"
    );
}

#[test]
fn test_discovery_finds_methods_and_functions() {
    let adapter = Python;
    let tree = parse::parse(&adapter.language(), SAMPLE).unwrap();
    let tests = adapter.test_functions(tree.root_node(), SAMPLE);

    let names: Vec<&str> = tests.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, vec!["test_async_method", "test_walrus_and_fstrings"]);
}
