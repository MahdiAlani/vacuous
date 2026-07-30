//! The Python grammar versions independently of tree-sitter core (0.25 against
//! 0.26 here) and the two meet through `tree-sitter-language`. When that breaks,
//! scans come back mysteriously empty, so fail here instead.

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
