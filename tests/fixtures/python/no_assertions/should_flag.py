"""All of these should be flagged. Call the thing, then stop."""


def test_creates_user():
    user = create_user("alice")
    user.save()


def test_empty_body():
    pass


def test_only_a_docstring():
    """Should exercise the happy path."""


def test_ellipsis_body():
    ...


def test_calls_and_prints():
    result = compute(2, 3)
    print(result)
