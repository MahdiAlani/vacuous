"""Assertions sitting after an early exit."""


def test_return_before_assert():
    user = create_user()
    return
    assert user.id is not None


def test_raise_before_assert():
    setup()
    raise RuntimeError("wip")
    assert compute() == 1


def test_return_in_loop_body_then_assert():
    for item in items():
        process(item)
        return
        assert item.done
