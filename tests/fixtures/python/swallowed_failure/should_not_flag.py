"""Handlers that leave the assertion able to fail."""

import pytest


def test_narrow_except_does_not_catch_assertions():
    # An AssertionError escapes `except ValueError`, so this still bites.
    try:
        assert parse("x") == 1
    except ValueError:
        pass


def test_handler_reraises():
    try:
        assert compute() == 2
    except Exception:
        raise


def test_handler_fails_explicitly():
    try:
        connect()
    except Exception:
        pytest.fail("connect() should not raise")


def test_finally_is_not_a_handler():
    try:
        assert compute() == 3
    finally:
        cleanup()


def test_assertion_after_the_try():
    # The handler does swallow, but the assertion is outside the try.
    try:
        value = compute()
    except Exception:
        value = None
    assert value == 4


def test_handler_records_the_failure_for_later():
    # An assertion inside a thread cannot fail the test on its own, so the
    # traceback is collected and asserted on afterwards. numpy does this in
    # test_callback.py and it is the correct pattern, not a swallowed failure.
    errors = []

    def runner():
        try:
            assert compute() == 42
        except Exception:
            errors.append(traceback.format_exc())

    run_in_threads(runner)
    assert not errors


def test_custom_exception_whose_name_contains_exception():
    # `TestingException` is not `Exception`. Matching the name by substring
    # instead of exactly reported this pydantic pattern as swallowed.
    try:
        assert recursively_defined_type_refs()
        raise TestingException
    except TestingException:
        pass
    assert not recursively_defined_type_refs()


def test_narrow_tuple_of_exceptions():
    try:
        assert compute() == 6
    except (ValueError, KeyError):
        pass


def test_dotted_exception_path():
    try:
        assert compute() == 7
    except asyncio.TimeoutError:
        pass


def test_handler_stores_the_exception():
    captured = None
    try:
        assert compute() == 5
    except Exception as exc:
        captured = exc
    assert captured is None
