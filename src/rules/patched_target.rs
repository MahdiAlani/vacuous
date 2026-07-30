//! The test mocks out the function it's named after, then only checks the mock:
//!
//! ```python
//! @patch("myapp.billing.charge_card")
//! def test_charge_card(mock_charge):
//!     charge_card(order)
//!     mock_charge.assert_called_once()
//! ```
//!
//! The real `charge_card` never runs. It passes because the mock was called, and
//! the mock was called because the test called it.

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
        // Patching a same-named dependency is routine. rich patches
        // `builtins.input` in `test_input` but asserts on real captured output;
        // requests patches `requests.help.idna` to set up the condition it's
        // testing. Both check real values, so neither is circular. Requiring
        // mock-only assertions is what separates those from the real thing.
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
