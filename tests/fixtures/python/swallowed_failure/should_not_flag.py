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
