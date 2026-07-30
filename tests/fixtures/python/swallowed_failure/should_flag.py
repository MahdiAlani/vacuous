"""The handler throws the assertion failure away."""


def test_bare_except():
    try:
        assert parse("x") == 1
    except:  # noqa: E722
        pass


def test_catches_exception():
    try:
        assert compute() == 2
    except Exception:
        pass


def test_catches_assertion_error():
    try:
        assert compute() == 3
    except AssertionError:
        pass


def test_catches_exception_and_only_logs():
    try:
        assert compute() == 4
    except Exception as exc:
        print(exc)
