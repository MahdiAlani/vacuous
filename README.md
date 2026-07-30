# vacuous

**Find the tests that cannot fail.**

When the same agent writes both the implementation and the tests, *"the tests pass"* stops being evidence of anything. Agents produce tests that assert nothing, assert on their own mocks, patch the very function they claim to test, or verify the framework instead of your code. Coverage counts them all as tested paths.

`vacuous` finds them. It runs locally in milliseconds. **No LLM, no API key, no network.**

> A *vacuous truth* is a claim that cannot be falsified. A vacuous test is one that passes no matter what your code does.

---

## Status: early

Python only, five rules shipped. Not yet published to PyPI or crates.io. See the [roadmap](#roadmap).

It already finds real bugs in mature codebases, though. From Flask's own test suite:

```python
@pytest.mark.parametrize("debug", [True, False])
@pytest.mark.parametrize("use_debugger", [True, False])
@pytest.mark.parametrize("use_reloader", [True, False])
@pytest.mark.parametrize("propagate_exceptions", [None, True, False])
def test_werkzeug_passthrough_errors(monkeypatch, debug, ...):
    rv = {}

    def run_simple_mock(*args, **kwargs):
        rv["passthrough_errors"] = kwargs.get("passthrough_errors")

    monkeypatch.setattr(werkzeug.serving, "run_simple", run_simple_mock)
    app.config["PROPAGATE_EXCEPTIONS"] = propagate_exceptions
    app.run(debug=debug, use_debugger=use_debugger, use_reloader=use_reloader)
    # ...and nothing ever asserts on rv.
```

That is **24 parametrized cases** that carefully capture a value and never check it.

---

## Usage

```console
$ vacuous check .

  10 vacuous tests in 347 (2.9%)

  tests/test_packages.py:4     no-assertions  certain
                              └─ `test_can_access_urllib3_attribute` contains no assertions — it passes unless the code under test raises.
  tests/test_requests.py:942   no-assertions  certain
                              └─ `test_decompress_gzip` contains no assertions — it passes unless the code under test raises.

  9 files scanned
```

```
vacuous check [PATH]              # defaults to the current directory
  --min-confidence <LEVEL>        # certain | likely | possible  (default: likely)
```

Exit codes: `0` clean, `1` findings, `2` vacuous itself failed. CI can therefore tell "your tests are bad" apart from "the tool is broken."

## Install

No releases yet. From source:

```console
git clone <repo> && cd vacuous
cargo build --release
./target/release/vacuous check /path/to/your/project
```

Requires Rust 1.88+.

---

## Design principles

**A false accusation is worse than a miss.** Every finding declares a confidence — `certain`, `likely`, or `possible` — and only `certain` and `likely` show by default. `certain` means a structural fact about the code, not a judgement call. When in doubt, `vacuous` stays quiet: it would rather miss a bad test than invent one.

That principle is enforced by the test suite. Every rule ships a paired `should_flag.py` / `should_not_flag.py` fixture, and the negative one is load-bearing — it *is* the false-positive contract. Both of the false-positive classes found while developing against real repositories are now permanent regression tests:

- **Nested functions.** Flask test suites define route handlers named `test` and click commands named `testcmd` *inside* tests. No runner collects them, so neither do we.
- **Local asserting helpers.** `common_object_test(app)` does the asserting for several Flask tests, and no name heuristic can tell. `vacuous` builds a per-file call graph and resolves helpers transitively instead of guessing from names.

**Deterministic.** Same input, same output, always. Nothing is sent anywhere.

## How it works

1. Walk the tree for files a test runner would actually collect (respecting `.gitignore`).
2. Parse each with [tree-sitter](https://tree-sitter.github.io/); extract test functions.
3. Run every rule against every test, in parallel across files.
4. Merge, sort by location, filter by confidence.

The architecture is deliberately boring and readable. The `LanguageAdapter` trait ([`src/lang/mod.rs`](src/lang/mod.rs)) is the only thing that knows about Python, so adding a language is additive rather than a rewrite. A rule ([`src/rules/`](src/rules/)) is a pure function from one parsed test to findings.

## Rules

| Rule | Catches | Confidence |
|---|---|---|
| `no-assertions` | A test body containing nothing that can fail | `certain` |
| `constant-assertion` | Every assertion is on literals — `assert True`, `assertEqual(3, 3)` | `certain` |
| `swallowed-failure` | The assertion is caught and discarded by a handler | `certain` |
| `unreachable-assertion` | The assertion sits after a `return` or `raise` | `certain` |
| `patched-target-under-test` | The test mocks its own subject, then asserts only on that mock | `likely` |

Three of these carry deliberate precision work that a naive version gets wrong:

- `constant-assertion` evaluates **truthiness**, not just constancy. `assert False, "should not get here"` is constant but always *fails* — a deliberate marker, and the opposite of vacuous.
- `swallowed-failure` checks **which exceptions are caught**. `except ValueError: pass` does not swallow an `AssertionError`.
- `unreachable-assertion` looks at **direct siblings only**. A `return` nested inside an `if` does not kill the statements after it.

## Validated against real code

Every rule is developed by running it over real repositories and hand-checking each finding. That process has caught seven distinct classes of false positive so far, all now pinned by regression fixtures — cross-file assertion helpers (`eq_`), decorator-supplied assertions (`@profiling.function_call_count`), benchmark fixtures, tests that return values to a harness, nested route handlers named `test`, same-file asserting helpers, and always-failing markers.

Current results across 29,395 tests in 12 well-maintained projects:

| Project | Vacuous | Tests | Rate | Time |
|---|---:|---:|---:|---:|
| sqlalchemy | 215 | 12,716 | 1.7% | 2.6 s |
| pydantic | 83 | 4,280 | 1.9% | 1.3 s |
| celery | 144 | 3,309 | 4.4% | 0.7 s |
| ansible | 61 | 2,300 | 2.7% | 1.6 s |
| scrapy | 53 | 2,005 | 2.6% | 0.7 s |
| django-rest-framework | 15 | 1,303 | 1.2% | 0.4 s |
| rich | 10 | 721 | 1.4% | 0.1 s |
| httpx | 2 | 539 | 0.4% | 0.1 s |
| click | 4 | 527 | 0.8% | 0.3 s |
| flask | 1 | 390 | 0.3% | 0.1 s |
| requests | 10 | 347 | 2.9% | 0.1 s |
| black | 1 | 258 | 0.4% | 0.2 s |
| **Total** | **599** | **29,395** | **2.0%** | |

Roughly **one test in fifty cannot fail**, in projects maintained to a high standard. Verified examples:

- **flask** — `test_werkzeug_passthrough_errors`, parametrized into 24 cases, records `rv["passthrough_errors"]` through a mock and never asserts on it.
- **celery** — `test_eager_chain_inside_task` calls `chain_add.apply_async(args=(4, 8), throw=True).get()` and never checks that the result is 12.
- **requests** — three tests in `test_packages.py` whose entire body is a bare attribute access, e.g. `requests.packages.urllib3`.

## Roadmap

**Static rules** — `mock-only-assertion`, `assert-on-mock-return` (asserts a value the test itself configured), `duplicate-test`, `broad-raises`, `skipped-test`, `smoke-only`.

**`vacuous verify`** — the deep pass. Stub a function's body, run the suite, and see if anything fails. If nothing does, no test guards that function. Scoped to a diff (`--since HEAD~1`) so it takes seconds rather than the hours that made classic mutation testing unadoptable. Reports a **guard rate**: the share of changed functions that any test actually protects.

**Adoption** — baseline/ratchet mode so existing repos fail only on *new* findings, `[tool.vacuous]` config in `pyproject.toml`, JSON and SARIF output, a pre-commit hook, and a GitHub Action.

**Distribution** — PyPI wheels (`uvx vacuous`), crates.io, prebuilt binaries.

**TypeScript/JavaScript** support.

## Prior art

Mutation testing ([Stryker](https://stryker-mutator.io/), [mutmut](https://github.com/boxed/mutmut), [cosmic-ray](https://github.com/sixty-north/cosmic-ray)) answers a similar question by running your suite against thousands of mutants. It is thorough and slow, which is why it never reached most projects. `vacuous` starts from the other end: an instant static pass that needs no test run at all, with targeted verification as an opt-in second step.

Academic test-smell detectors (tsDetect, DARTS) identify overlapping patterns but are IDE plugins targeting Java.

## License

MIT OR Apache-2.0
