"""None of these may be flagged."""

import pytest
from unittest.mock import Mock


def test_asserts_on_result():
    assert add(1, 2) == 3


def test_mixes_constant_and_real():
    # A stray `assert True` next to a real check is untidy, not vacuous.
    assert True
    assert add(1, 2) == 3


def test_mock_assertion_with_literal_args():
    # Literal argument, but it checks recorded behaviour.
    sink = Mock()
    emit(sink, 42)
    sink.assert_called_once_with(42)


def test_assertion_message_is_a_literal():
    # Only the condition counts, not the message.
    assert compute() == 7, "compute() should return 7"


def test_raises_with_literal():
    with pytest.raises(ValueError):
        parse("nope")


def test_asserts_on_variable():
    expected = build_expected()
    assert actual() == expected


def test_assert_false_is_a_failure_marker():
    # Constant, but it always fails, so it's a marker rather than a tautology.
    try:
        inspect(thing)
    except Exception as exc:
        assert False, f"should not have raised {exc}"


def test_always_failing_comparison():
    assert 1 == 2


def test_assert_false_alongside_a_real_check():
    if not ready():
        assert False, "never ready"
    assert compute() == 5
