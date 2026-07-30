//! Rule: `unreachable-assertion` — the assertion sits after an early exit.
//!
//! ```python
//! def test_user_is_saved():
//!     user = create_user()
//!     return
//!     assert user.id is not None
//! ```
//!
//! A stray `return` (or `raise`) leaves everything after it dead. Agents produce
//! this when editing a test in several passes, and it is invisible in a test
//! report because the test still passes.
//!
//! Only statements *directly* after the exit in the *same* block count. A
//! `return` nested inside an `if` does not make its siblings unreachable, and
//! this rule must not claim otherwise.

use super::{Rule, RuleCtx};
use crate::parse::{descendants, line_of};
use crate::report::{Confidence, Finding};

pub struct UnreachableAssertion;

impl Rule for UnreachableAssertion {
    fn name(&self) -> &'static str {
        "unreachable-assertion"
    }

    fn check(&self, ctx: &RuleCtx) -> Vec<Finding> {
        let mut findings = Vec::new();

        // Any node may be a statement container. We inspect direct children
        // only, which is what makes "same block" precise without naming a
        // language-specific block node.
        for container in descendants(ctx.test.body) {
            let mut cursor = container.walk();
            let statements: Vec<_> = container.named_children(&mut cursor).collect();

            let Some(exit_index) = statements
                .iter()
                .position(|statement| ctx.adapter.is_terminating_statement(*statement))
            else {
                continue;
            };

            for dead in &statements[exit_index + 1..] {
                if let Some(assertion) =
                    descendants(*dead).find(|node| ctx.adapter.is_assertion(*node, ctx.src))
                {
                    findings.push(Finding {
                        rule: self.name(),
                        confidence: Confidence::Certain,
                        file: ctx.path.to_path_buf(),
                        line: line_of(assertion),
                        test_name: ctx.test.name.clone(),
                        message: format!(
                            "this assertion in `{}` is unreachable — line {} exits the test first.",
                            ctx.test.name,
                            line_of(statements[exit_index])
                        ),
                    });
                }
            }
        }

        findings
    }
}
