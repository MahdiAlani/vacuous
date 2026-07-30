//! The language abstraction.
//!
//! This trait is the seam that keeps adding a language additive rather than a
//! rewrite. A rule is written once against `LanguageAdapter`; supporting
//! TypeScript later means implementing this trait, not touching the rules.

use std::collections::HashSet;
use std::path::Path;
use tree_sitter::Node;

pub mod python;

/// A test function we found in a test file.
#[derive(Debug, Clone)]
pub struct TestFn<'t> {
    /// e.g. `test_user_can_log_in`
    pub name: String,
    /// The whole function definition node.
    pub node: Node<'t>,
    /// The function's body block — what rules actually inspect.
    pub body: Node<'t>,
    /// 1-based line of the `def`.
    pub line: usize,
}

pub trait LanguageAdapter: Send + Sync {
    /// Short id, e.g. `python`.
    fn name(&self) -> &'static str;

    /// The tree-sitter grammar for this language.
    fn language(&self) -> tree_sitter::Language;

    /// Would a test runner collect this file? Mirrors the runner's own
    /// discovery rules so we don't scan helper modules and report noise.
    fn is_test_file(&self, path: &Path) -> bool;

    /// Every test function in the file.
    fn test_functions<'t>(&self, root: Node<'t>, src: &str) -> Vec<TestFn<'t>>;

    /// Is this node something that can make the test fail?
    ///
    /// Deliberately broad: a mock assertion like `mock.assert_called_once()`
    /// counts, because it genuinely *can* fail. Judging whether such an
    /// assertion is *meaningful* is a separate rule's job, not this one's.
    fn is_assertion(&self, node: Node<'_>, src: &str) -> bool;

    /// Does this node look like a call to a helper that asserts on our behalf?
    ///
    /// A pure name heuristic, used only as a fallback for helpers we cannot
    /// resolve — for example one imported from another module. Prefer
    /// [`LanguageAdapter::asserting_helpers`], which resolves for real.
    fn is_verification_helper(&self, node: Node<'_>, src: &str) -> bool;

    /// If `node` is a call, the final segment of the called function's name.
    fn called_name<'a>(&self, node: Node<'_>, src: &'a str) -> Option<&'a str>;

    /// Names of functions defined in this file whose bodies contain assertions,
    /// resolved transitively.
    ///
    /// This is what lets us stay quiet on the extremely common pattern of a
    /// shared checker in the same test module:
    ///
    /// ```python
    /// def common_object_test(app):      # asserts
    ///     assert app.secret_key == "config"
    ///
    /// def test_config_from_pyfile():    # no assertions of its own, but fine
    ///     common_object_test(app)
    /// ```
    ///
    /// Name heuristics cannot catch `common_object_test`, so we build a
    /// per-file call graph instead.
    fn asserting_helpers(&self, root: Node<'_>, src: &str) -> HashSet<String>;

    /// Is the body effectively empty — only `pass`, `...`, or a docstring?
    fn is_empty_body(&self, body: Node<'_>, src: &str) -> bool;
}
