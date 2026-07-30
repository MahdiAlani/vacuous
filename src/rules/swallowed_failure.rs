//! An assertion whose failure gets caught and thrown away:
//!
//! ```python
//! def test_parses_config():
//!     try:
//!         assert parse(raw) == expected
//!     except Exception:
//!         pass
//! ```
//!
//! `except ValueError: pass` is fine, since an `AssertionError` escapes it.
//! [`crate::lang::LanguageAdapter::is_swallowing_handler`] draws that line.

use super::{Rule, RuleCtx};
use crate::parse::{descendants, line_of};
use crate::report::{Confidence, Finding};

pub struct SwallowedFailure;

impl Rule for SwallowedFailure {
    fn name(&self) -> &'static str {
        "swallowed-failure"
    }

    fn check(&self, ctx: &RuleCtx) -> Vec<Finding> {
        let mut findings = Vec::new();

        for handler in descendants(ctx.test.body)
            .filter(|node| ctx.adapter.is_swallowing_handler(*node, ctx.src))
        {
            let Some(guarded) = handler.parent() else {
                continue;
            };

            // Find assertions protected by this handler. We use byte positions
            // rather than node kinds so the rule stays language-agnostic: the
            // guarded body always precedes its handler in source order.
            let smothered = descendants(guarded).find(|node| {
                node.end_byte() <= handler.start_byte()
                    && node.start_byte() >= guarded.start_byte()
                    && ctx.adapter.is_assertion(*node, ctx.src)
            });

            if let Some(assertion) = smothered {
                findings.push(Finding {
                    rule: self.name(),
                    confidence: Confidence::Certain,
                    file: ctx.path.to_path_buf(),
                    line: line_of(assertion),
                    test_name: ctx.test.name.clone(),
                    message: format!(
                        "this assertion in `{}` is caught and discarded by the handler on line {} — it can never fail the test.",
                        ctx.test.name,
                        line_of(handler)
                    ),
                });
            }
        }

        findings
    }
}
