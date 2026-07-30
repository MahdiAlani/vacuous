//! Everything language-specific sits behind `LanguageAdapter`, so the checks in
//! `crate::rules` don't need to know what they're looking at.

use std::collections::HashSet;
use std::path::Path;
use tree_sitter::Node;

pub mod python;

#[derive(Debug, Clone)]
pub struct TestFn<'t> {
    pub name: String,
    /// The whole function definition.
    pub node: Node<'t>,
    /// The body block, which is what checks look at.
    pub body: Node<'t>,
    /// 1-based line of the `def`.
    pub line: usize,
}

pub trait LanguageAdapter: Send + Sync {
    fn name(&self) -> &'static str;

    fn language(&self) -> tree_sitter::Language;

    /// Would a test runner collect this file?
    fn is_test_file(&self, path: &Path) -> bool;

    fn test_functions<'t>(&self, root: Node<'t>, src: &str) -> Vec<TestFn<'t>>;

    /// Anything that can make the test fail. Mock assertions count here;
    /// whether they're meaningful is a different question.
    fn is_assertion(&self, node: Node<'_>, src: &str) -> bool;

    /// Name-based guess for helpers we can't resolve, e.g. imported from
    /// another module. Only used to suppress findings.
    fn is_verification_helper(&self, node: Node<'_>, src: &str) -> bool;

    /// Final segment of a call's callee, if this is a call at all.
    fn called_name<'a>(&self, node: Node<'_>, src: &'a str) -> Option<&'a str>;

    /// Functions in this file that assert, followed through calls. Catches the
    /// common shape of a shared checker living in the same test module.
    fn asserting_helpers(&self, root: Node<'_>, src: &str) -> HashSet<String>;

    /// Whether an assertion with a fixed outcome always passes.
    ///
    /// `Some(false)` is an always-failing marker such as `assert False`, which
    /// isn't our problem — it fails loudly by itself. `None` means the outcome
    /// depends on the code under test.
    fn constant_assertion_outcome(&self, node: Node<'_>, src: &str) -> Option<bool>;

    /// An assertion about a mock's recorded calls rather than a real value.
    fn is_mock_assertion(&self, node: Node<'_>, src: &str) -> bool;

    /// Symbols this test swaps out for a mock, from decorators and body.
    fn patched_symbols(&self, test: &TestFn<'_>, src: &str) -> Vec<String>;

    /// `test_charge_card` -> `charge_card`.
    fn implied_subject(&self, test_name: &str) -> Option<String>;

    /// Whether this handler would swallow a failed assertion. `except
    /// ValueError` wouldn't, since an assertion failure escapes it.
    fn is_swallowing_handler(&self, node: Node<'_>, src: &str) -> bool;

    /// Ends the function, so later statements in the same block are dead.
    fn is_terminating_statement(&self, node: Node<'_>) -> bool;

    /// Might a decorator be doing the asserting? Unfamiliar decorators should
    /// count as suspect — guessing wrong here produces false positives.
    fn assertions_may_come_from_decorator(&self, test: &TestFn<'_>, src: &str) -> bool;

    /// Only `pass`, `...`, or a docstring.
    fn is_empty_body(&self, body: Node<'_>, src: &str) -> bool;

    /// Hands a value back, which means something else is driving the test.
    fn body_returns_value(&self, body: Node<'_>) -> bool;
}
