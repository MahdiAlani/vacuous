# vacuous

Finds Python tests that pass no matter what your code does.

A test with no assertions, or one that only checks its own mocks, still shows up
green and still counts as coverage. Those are the worst kind to have, because
they cost you the thing tests are for — you find out the code is broken from
somewhere other than your test suite.

```console
$ vacuous check tests/

  12 vacuous tests in 847 (1.4%)

  tests/test_billing.py:41   no-assertions              certain
                             └─ `test_refund_is_recorded` contains no assertions — it passes unless the code under test raises.
  tests/test_users.py:118    swallowed-failure          certain
                             └─ this assertion in `test_email_is_unique` is caught and discarded by the handler on line 121 — it can never fail the test.
  tests/test_orders.py:92    patched-target-under-test  likely
                             └─ `test_charge_card` replaces `charge_card` with a mock and then only asserts on that mock — the real `charge_card` never runs.

  63 files scanned
```

It's a static analysis pass over tree-sitter, so it doesn't run your tests and
doesn't need your dependencies installed. Whole repos take well under a second.
No network, no API keys.

## Install

```console
uv tool install vacuous
```

or `pipx install vacuous`, or `pip install vacuous`. There are prebuilt wheels
for Linux, macOS and Windows, so this needs no Rust toolchain.

If you'd rather build it yourself:

```console
cargo install vacuous
```

Needs Rust 1.88 or newer.

## Usage

```
vacuous check [PATH]              # defaults to the current directory
  --min-confidence <LEVEL>        # certain | likely | possible  (default: likely)
  --format <FORMAT>               # pretty | json | sarif        (default: pretty)
  --baseline <FILE>               # defaults to .vacuous-baseline.json if present
  --no-baseline                   # report everything, baseline included

vacuous baseline [PATH]           # record what's there today
```

Exits `0` when clean, `1` when it finds something, `2` if the tool itself fell
over — so CI can tell a real failure apart from a broken run.

## Adding it to an existing project

Any codebase of any age will have findings already, and nobody is going to fix
two hundred of them before their next commit. Record them once:

```console
$ vacuous baseline
vacuous: recorded 144 findings in .vacuous-baseline.json
vacuous: `vacuous check` will now report only new ones
```

Commit that file. From then on `vacuous check` passes, and only fails when
someone adds a *new* test that can't fail.

Entries are keyed on the rule, file and test name, deliberately not the line
number, so editing a file doesn't invalidate the baseline. When findings get
fixed, `vacuous` says how many entries are stale and you can re-record.

## pre-commit

```yaml
repos:
  - repo: https://github.com/MahdiAlani/vacuous-pre-commit
    rev: v0.1.1
    hooks:
      - id: vacuous
```

Installs from the PyPI wheel, so contributors don't need Rust.

## CI

```yaml
- run: vacuous check --format sarif > vacuous.sarif
  continue-on-error: true
- uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: vacuous.sarif
```

Findings then show up as annotations on the pull request. `certain` maps to
`error`, `likely` to `warning`, `possible` to `note`; GitHub won't block merges
on any of them unless you configure it to.

## Checks

| Check | What it means |
|---|---|
| `no-assertions` | Nothing in the test body can fail |
| `constant-assertion` | Every assertion is on literals: `assert True`, `assertEqual(3, 3)` |
| `swallowed-failure` | An `except` block discards the assertion |
| `unreachable-assertion` | The assertion sits after a `return` or `raise` |
| `patched-target-under-test` | The test mocks the function it's named for, then only checks the mock |

Each finding carries a confidence. `certain` means it's a structural fact about
the code; `likely` means there's a judgement call involved. Only `certain` and
`likely` show by default.

`no-assertions` is graded rather than fixed, because it catches two different
things. A body of `pass` or a lone docstring is a stub and gets `certain`. A body
that calls real code but asserts nothing gets `likely`, because it might be a
deliberate crash-or-hang regression test — numpy has several, and that's the
right way to test "this used to segfault". Nothing static can tell those apart,
so `--min-confidence certain` gives you only the indefensible ones.

## What it deliberately ignores

Tests without assertions aren't automatically bad, which is the main reason
[pytest hasn't shipped a flag for this](https://github.com/pytest-dev/pytest/issues/2706).
Getting the exceptions right is most of the work here, so `vacuous` stays quiet on:

- Tests that delegate to a helper that asserts, including one defined elsewhere
  in the file and reached indirectly. Flask's `common_object_test(app)` is the
  usual shape.
- Assertion helpers that don't look like assertions — `eq_`, `is_`, `ne_` and the
  rest of the nose/SQLAlchemy family.
- Anything where a decorator does the asserting, like SQLAlchemy's
  `@profiling.function_call_count()` or a `@pytest.mark.benchmark` handing off to
  the benchmark fixture. Plain `pytest.mark` labels don't count.
- Tests returning a value, which means something else is driving them.
- Functions nested inside tests. Flask names route handlers `test` and Click
  commands `testcmd`; no runner collects those.
- `assert False, "shouldn't get here"`. That's constant but always *fails*, so
  it's a deliberate marker, not a vacuous test. Ruff's
  [PT015](https://docs.astral.sh/ruff/rules/pytest-assert-always-false/) and
  [B011](https://docs.astral.sh/ruff/rules/assert-false/) cover that case from
  the other direction.
- `except ValueError: pass` around an assertion. An `AssertionError` escapes it,
  so nothing is being swallowed.

Most of those came from running it over real suites and finding out it was wrong.
It's currently checked against flask, requests, httpx, rich, click, black,
scrapy, celery, pydantic, ansible, django-rest-framework and sqlalchemy — about
29,000 tests — with every finding read by hand. Roughly 2% of tests in those
projects can't fail.

If you hit a false positive, that's a bug worth filing.

## How it works

Walk for files a test runner would collect, respecting `.gitignore`. Parse each
one, pull out the test functions, run every check over them, sort by location.
Files are handled in parallel.

The Python-specific parts live behind one trait in
[`src/lang/mod.rs`](src/lang/mod.rs), so the checks themselves don't know what
language they're looking at. A check is a function from one parsed test to a list
of findings; they're in [`src/rules/`](src/rules/). Every check has a pair of
fixtures under `tests/fixtures/`, one that should be flagged and one that
shouldn't.

## Related tools

- [mutmut](https://github.com/boxed/mutmut) and
  [cosmic-ray](https://github.com/sixty-north/cosmic-ray) answer a harder version
  of this question by mutating your code and re-running the suite. Far more
  thorough, and much slower. Worth reaching for when you want a real answer about
  a specific module.
- [flake8-aaa](https://github.com/jamescooke/flake8-aaa) lints tests for
  Arrange-Act-Assert structure, which catches some of the same shapes.
- [ruff](https://docs.astral.sh/ruff/) has a
  [pytest ruleset](https://docs.astral.sh/ruff/rules/#flake8-pytest-style-pt)
  worth turning on regardless, and covers always-false assertions.
- [flake8-pytest-style](https://github.com/m-burst/flake8-pytest-style) for
  pytest idioms more broadly.

`vacuous` is meant to sit alongside these, not replace them.

## Planned

Config in `pyproject.toml`, so per-project settings live where the rest of your
tooling is configured. Per-line suppression comments.

After that, a `verify` mode: stub out a function, run the tests, and see whether
anything notices. Scoped to a diff it's quick enough to be useful, and it answers
directly what this tool can only approximate by reading code.

TypeScript, eventually. The checks don't know they're looking at Python — that
lives behind one trait — so it's a new adapter rather than a rewrite.

## License

MIT or Apache-2.0, at your option.
