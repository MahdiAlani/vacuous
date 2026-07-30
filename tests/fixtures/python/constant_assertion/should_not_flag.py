"""No test here may be flagged as `constant-assertion`."""

import pytest
from unittest.mock import Mock


def test_asserts_on_result():
    assert add(1, 2) == 3


def test_mixes_constant_and_real():
    # A stray `assert True` next to a real check is untidy, not vacuous.
    # Flagging this would be exactly the noise that gets a linter uninstalled.
    assert True
    assert add(1, 2) == 3


def test_mock_assertion_with_literal_args():
    # Literal argument, but this asserts recorded behaviour rather than a
    # tautology, so it is a real check.
    sink = Mock()
    emit(sink, 42)
    sink.assert_called_once_with(42)


def test_assertion_message_is_a_literal():
    # The *message* is constant; the condition is not. Only the condition counts.
    assert compute() == 7, "compute() should return 7"


def test_raises_with_literal():
    with pytest.raises(ValueError):
        parse("nope")


def test_asserts_on_variable():
    expected = build_expected()
    assert actual() == expected


# --- Regression cases found by scanning rich ---------------------------------


def test_assert_false_is_a_failure_marker():
    # `assert False` is constant but always *fails*, so it is the opposite of
    # vacuous. Taken from rich/tests/test_inspect.py. Flagging this was the bug
    # that motivated evaluating truthiness rather than mere constancy.
    try:
        inspect(thing)
    except Exception as exc:
        assert False, f"should not have raised {exc}"


def test_always_failing_comparison():
    # Same idea without the marker idiom: this can only ever fail.
    assert 1 == 2


def test_assert_false_alongside_a_real_check():
    if not ready():
        assert False, "never ready"
    assert compute() == 5
