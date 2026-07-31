//! A check is a function from one parsed test to a list of findings. Adding one
//! means a file here, a line in [`all_rules`], and a pair of fixtures under
//! `tests/fixtures`. Write the `should_not_flag` half first.

use std::collections::HashSet;
use std::path::Path;

use crate::lang::{LanguageAdapter, TestFn};
use crate::report::Finding;

mod constant_assertion;
mod no_assertions;
mod patched_target;
mod swallowed_failure;
mod unreachable_assertion;

/// Work shared by every test in a file. Separate from [`RuleCtx`] because
/// building it is O(file), and doing that per test would be O(file × tests).
pub struct FileCtx {
    pub asserting_helpers: HashSet<String>,
}

pub struct RuleCtx<'a, 't> {
    pub src: &'a str,
    pub path: &'a Path,
    pub adapter: &'a dyn LanguageAdapter,
    pub file: &'a FileCtx,
    pub test: &'a TestFn<'t>,
}

pub trait Rule: Send + Sync {
    /// Stable id, used in output and config.
    fn name(&self) -> &'static str;

    /// One line, for `--format sarif` and docs.
    fn description(&self) -> &'static str;

    fn check(&self, ctx: &RuleCtx) -> Vec<Finding>;
}

/// Order here is execution order only; output is sorted by location.
pub fn all_rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(no_assertions::NoAssertions),
        Box::new(constant_assertion::ConstantAssertion),
        Box::new(patched_target::PatchedTargetUnderTest),
        Box::new(swallowed_failure::SwallowedFailure),
        Box::new(unreachable_assertion::UnreachableAssertion),
    ]
}
