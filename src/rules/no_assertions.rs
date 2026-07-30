//! A test body with nothing in it that can fail. It passes for exactly one
//! reason: the code didn't raise. Coverage counts it either way.
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

        let message = if ctx.adapter.is_empty_body(ctx.test.body, ctx.src) {
            format!("`{}` has an empty body — it can never fail.", ctx.test.name)
        } else {
            format!(
                "`{}` contains no assertions — it passes unless the code under test raises.",
                ctx.test.name
            )
        };

        vec![Finding {
            rule: self.name(),
            confidence: Confidence::Certain,
            file: ctx.path.to_path_buf(),
            line: ctx.test.line,
            test_name: ctx.test.name.clone(),
            message,
        }]
    }
}
