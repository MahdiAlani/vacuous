//! A test body with nothing in it that can fail. It passes for exactly one
//! reason: the code didn't raise. Coverage counts it either way.
//!
//! Two quite different things get caught here, so they are graded differently.
//! A body of `pass` or a lone docstring is a stub, and nothing defends it.
//! A body that calls real code but asserts nothing might be a deliberate
//! crash-or-hang regression test — numpy has several, and they are the right
//! way to test "this used to segfault". Statically the two are
//! indistinguishable, so the second only ever gets `likely`.
//!
//! Most of what follows is the four ways this conclusion can be wrong.

use super::{Rule, RuleCtx};
use crate::parse::descendants;
use crate::report::{Confidence, Finding};

pub struct NoAssertions;

impl Rule for NoAssertions {
    fn name(&self) -> &'static str {
        "no-assertions"
    }

    fn description(&self) -> &'static str {
        "The test contains nothing that can fail, so it passes unless the code raises."
    }

    fn check(&self, ctx: &RuleCtx) -> Vec<Finding> {
        let mut delegates_to_helper = false;

        for node in descendants(ctx.test.body) {
            if ctx.adapter.is_assertion(node, ctx.src) {
                return Vec::new();
            }

            // False-positive guard 1 (precise): the test calls a helper defined
            // in this file that we have resolved as asserting.
            if let Some(callee) = ctx.adapter.called_name(node, ctx.src)
                && ctx.file.asserting_helpers.contains(callee)
            {
                delegates_to_helper = true;
            }

            // False-positive guard 2 (heuristic): the helper is imported from
            // elsewhere so we cannot resolve it, but its name says it checks
            // something. We would rather miss a bad test than invent one.
            if ctx.adapter.is_verification_helper(node, ctx.src) {
                delegates_to_helper = true;
            }
        }

        if delegates_to_helper {
            return Vec::new();
        }

        // False-positive guard 3: the test returns a value, so a decorator is
        // driving it and asserting on our behalf.
        if ctx.adapter.body_returns_value(ctx.test.body) {
            return Vec::new();
        }

        // False-positive guard 4: a decorator may be doing the asserting —
        // profiling call-count limits, benchmark regression checks, and so on.
        if ctx
            .adapter
            .assertions_may_come_from_decorator(ctx.test, ctx.src)
        {
            return Vec::new();
        }

        let (confidence, message) = if ctx.adapter.is_empty_body(ctx.test.body, ctx.src) {
            (
                Confidence::Certain,
                format!("`{}` has an empty body — it can never fail.", ctx.test.name),
            )
        } else {
            (
                Confidence::Likely,
                format!(
                    "`{}` runs code but asserts nothing — it only fails if that code raises.",
                    ctx.test.name
                ),
            )
        };

        vec![Finding {
            rule: self.name(),
            confidence,
            file: ctx.path.to_path_buf(),
            line: ctx.test.line,
            test_name: ctx.test.name.clone(),
            test_line: ctx.test.line,
            message,
        }]
    }
}
