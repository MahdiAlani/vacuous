//! The rule engine.
//!
//! A rule is a pure function from one parsed test to zero or more findings.
//! Adding a rule means: one file here, one line in [`all_rules`], and a paired
//! `should_flag` / `should_not_flag` fixture. The negative fixture is the
//! important one — it encodes the false-positive guarantee.

use std::collections::HashSet;
use std::path::Path;

use crate::lang::{LanguageAdapter, TestFn};
use crate::report::Finding;

mod no_assertions;

/// Analysis that is computed once per file and shared by every test in it.
///
/// Keeping this separate from [`RuleCtx`] matters for performance: resolving the
/// call graph is O(file), and doing it per-test would make it O(file × tests).
pub struct FileCtx {
    /// Functions defined in this file that assert, transitively.
    /// See [`crate::lang::LanguageAdapter::asserting_helpers`].
    pub asserting_helpers: HashSet<String>,
}

/// Everything a rule is allowed to look at.
pub struct RuleCtx<'a, 't> {
    pub src: &'a str,
    pub path: &'a Path,
    pub adapter: &'a dyn LanguageAdapter,
    /// Per-file analysis, shared across all tests in the file.
    pub file: &'a FileCtx,
    pub test: &'a TestFn<'t>,
}

pub trait Rule: Send + Sync {
    /// Stable id used in output, config, and suppression comments.
    fn name(&self) -> &'static str;

    fn check(&self, ctx: &RuleCtx) -> Vec<Finding>;
}

/// The registry. Order here is the order rules run; it does not affect output
/// ordering, which is sorted by location in [`crate::report::Report::sort`].
pub fn all_rules() -> Vec<Box<dyn Rule>> {
    vec![Box::new(no_assertions::NoAssertions)]
}
