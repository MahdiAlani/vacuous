# vacuous

**Find the tests that cannot fail.**

When the same agent writes both the implementation and the tests, *"the tests pass"* stops being evidence of anything. Agents produce tests that assert nothing, assert on their own mocks, patch the very function they claim to test, or verify the framework instead of your code. Coverage counts them all as tested paths.

`vacuous` finds them. It runs locally in milliseconds. **No LLM, no API key, no network.**

> A *vacuous truth* is a claim that cannot be falsified. A vacuous test is one that passes no matter what your code does.

---

## Status: early

Python only, and one rule shipped so far. Not yet published to PyPI or crates.io. See the [roadmap](#roadmap).

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

## Roadmap

**Static rules** — `constant-assertion` (`assert True`), `patched-target-under-test` (the test patches the function it tests), `mock-only-assertion`, `assert-on-mock-return` (asserts a value the test itself configured), `swallowed-failure` (`try: assert ... except: pass`), `unreachable-assertion`, `duplicate-test`, `broad-raises`, `skipped-test`.

**`vacuous verify`** — the deep pass. Stub a function's body, run the suite, and see if anything fails. If nothing does, no test guards that function. Scoped to a diff (`--since HEAD~1`) so it takes seconds rather than the hours that made classic mutation testing unadoptable. Reports a **guard rate**: the share of changed functions that any test actually protects.

**Adoption** — baseline/ratchet mode so existing repos fail only on *new* findings, `[tool.vacuous]` config in `pyproject.toml`, JSON and SARIF output, a pre-commit hook, and a GitHub Action.

**Distribution** — PyPI wheels (`uvx vacuous`), crates.io, prebuilt binaries.

**TypeScript/JavaScript** support.

## Prior art

Mutation testing ([Stryker](https://stryker-mutator.io/), [mutmut](https://github.com/boxed/mutmut), [cosmic-ray](https://github.com/sixty-north/cosmic-ray)) answers a similar question by running your suite against thousands of mutants. It is thorough and slow, which is why it never reached most projects. `vacuous` starts from the other end: an instant static pass that needs no test run at all, with targeted verification as an opt-in second step.

Academic test-smell detectors (tsDetect, DARTS) identify overlapping patterns but are IDE plugins targeting Java.

## License

MIT OR Apache-2.0
