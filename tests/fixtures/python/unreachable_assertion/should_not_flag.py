"""Early exits that leave the assertions perfectly reachable."""


def test_conditional_return():
    # The `return` is nested inside an `if`, so it does not kill its siblings.
    # A rule that looked at descendants instead of direct children would get
    # this wrong.
    if not feature_enabled():
        return
    assert compute() == 1


def test_assert_before_return():
    assert compute() == 2
    return


def test_return_inside_loop():
    for item in items():
        if item.bad:
            return
    assert all_processed()


def test_raise_in_except_only():
    try:
        value = compute()
    except Exception:
        raise
    assert value == 3
