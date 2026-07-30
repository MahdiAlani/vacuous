"""Handlers that do not swallow assertion failures."""

import pytest


def test_narrow_except_does_not_catch_assertions():
    # An AssertionError escapes `except ValueError`, so the assertion still
    # bites. This is the precision case that separates us from a naive
    # "assert inside try" check.
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
    # The handler swallows, but the assertion sits outside the try entirely.
    try:
        value = compute()
    except Exception:
        value = None
    assert value == 4
