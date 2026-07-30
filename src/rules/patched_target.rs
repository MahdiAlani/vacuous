//! Rule: `patched-target-under-test` — the test mocks the thing it is named for.
//!
//! ```python
//! @patch("myapp.billing.charge_card")
//! def test_charge_card(mock_charge):
//!     charge_card(order)
//!     mock_charge.assert_called_once()
//! ```
//!
//! The real `charge_card` never runs. The test passes if the mock was called,
//! which it was, because the test called it. This is one of the most common
//! shapes in agent-written suites and one of the most misleading, because it
//! looks rigorous and reports as covered.

use super::{Rule, RuleCtx};
use crate::parse::descendants;
use crate::report::{Confidence, Finding};

pub struct PatchedTargetUnderTest;

/// Below this length, a symbol name matches by coincidence too often to trust.
const MIN_SYMBOL_LEN: usize = 3;

impl Rule for PatchedTargetUnderTest {
    fn name(&self) -> &'static str {
        "patched-target-under-test"
    }

    fn check(&self, ctx: &RuleCtx) -> Vec<Finding> {
        let Some(subject) = ctx.adapter.implied_subject(&ctx.test.name) else {
            return Vec::new();
        };

        // Requirement 1: the test mocks out exactly what it is named for.
        let patched = ctx.adapter.patched_symbols(ctx.test, ctx.src);
        let Some(symbol) = patched
            .iter()
            .find(|symbol| symbol.len() >= MIN_SYMBOL_LEN && **symbol == subject)
        else {
            return Vec::new();
        };

        // Requirement 2: every assertion is about a mock.
        //
        // This second condition is what makes the rule trustworthy. Patching a
        // same-named dependency is completely normal — rich patches
        // `builtins.input` inside `test_input` while asserting on real captured
        // output, and requests patches `requests.help.idna` to *create* the
        // condition under test. Both assert on real values, so both are excluded
        // here. What is left is the genuinely circular case: the subject was
        // replaced by a mock, and nothing but that mock was ever checked.
        let mut assertions = 0usize;
        let mut mock_assertions = 0usize;
        for node in descendants(ctx.test.body) {
            if !ctx.adapter.is_assertion(node, ctx.src) {
                continue;
            }
            assertions += 1;
            if ctx.adapter.is_mock_assertion(node, ctx.src) {
                mock_assertions += 1;
            }
        }

        // No assertions at all is `no-assertions`' business.
        if assertions == 0 || mock_assertions != assertions {
            return Vec::new();
        }

        vec![Finding {
            rule: self.name(),
            // `Likely` rather than `Certain`: the link between a test's name and
            // its subject is a naming convention, not a structural fact.
            confidence: Confidence::Likely,
            file: ctx.path.to_path_buf(),
            line: ctx.test.line,
            test_name: ctx.test.name.clone(),
            message: format!(
                "`{}` replaces `{symbol}` with a mock and then only asserts on that mock — the real `{symbol}` never runs.",
                ctx.test.name
            ),
        }]
    }
}
