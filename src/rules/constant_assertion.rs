//! Rule: `constant-assertion` — the test only asserts on literals.
//!
//! `assert True`, `assert 1 == 1`, `self.assertEqual(2, 2)`. Nothing from the
//! code under test appears in the assertion, so it holds regardless of what the
//! code does.
//!
//! We flag only when *every* assertion in the test is constant. A stray
//! `assert True` alongside real checks is untidy, not vacuous, and reporting it
//! would be the sort of noise that gets a linter uninstalled.

use super::{Rule, RuleCtx};
use crate::parse::{descendants, line_of, snippet};
use crate::report::{Confidence, Finding};

pub struct ConstantAssertion;

impl Rule for ConstantAssertion {
    fn name(&self) -> &'static str {
        "constant-assertion"
    }

    fn check(&self, ctx: &RuleCtx) -> Vec<Finding> {
        let mut total = 0usize;
        let mut always_passes = 0usize;
        let mut first_offender = None;

        for node in descendants(ctx.test.body) {
            if !ctx.adapter.is_assertion(node, ctx.src) {
                continue;
            }
            total += 1;

            // `Some(false)` — an always-failing marker like `assert False` — is
            // counted as a real assertion, because it will fail the test loudly.
            if ctx.adapter.constant_assertion_outcome(node, ctx.src) == Some(true) {
                always_passes += 1;
                if first_offender.is_none() {
                    first_offender = Some(node);
                }
            }
        }

        // `total == 0` is `no-assertions`' business, not ours.
        if total == 0 || always_passes != total {
            return Vec::new();
        }

        let Some(offender) = first_offender else {
            return Vec::new();
        };

        let message = if total == 1 {
            format!(
                "`{}` only asserts on constants — `{}` holds however the code behaves.",
                ctx.test.name,
                snippet(offender, ctx.src, 48)
            )
        } else {
            format!(
                "all {total} assertions in `{}` are on constants — e.g. `{}`, which holds however the code behaves.",
                ctx.test.name,
                snippet(offender, ctx.src, 48)
            )
        };

        vec![Finding {
            rule: self.name(),
            confidence: Confidence::Certain,
            file: ctx.path.to_path_buf(),
            line: line_of(offender),
            test_name: ctx.test.name.clone(),
            message,
        }]
    }
}
