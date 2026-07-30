"""No test in this file may be flagged.

This fixture is the false-positive contract for `no-assertions`. Every
assertion dialect we claim to understand gets a case here, including the two
suppression routes: delegation to a verification helper, and mock assertions
(which genuinely can fail, so this rule must leave them alone).
"""

import numpy as np
import pytest
from hamcrest import assert_that, equal_to
from unittest.mock import Mock


def test_bare_assert():
    assert add(1, 2) == 3


def test_pytest_raises():
    with pytest.raises(ValueError):
        parse("not a number")


def test_numpy_allclose():
    np.testing.assert_allclose(transform([1.0]), [2.0])


def test_hamcrest():
    assert_that(add(1, 1), equal_to(2))


def test_mock_assertion():
    sink = Mock()
    emit(sink, 42)
    sink.assert_called_once_with(42)


def test_delegates_to_assert_helper():
    response = fetch("/health")
    _assert_healthy(response)


def test_delegates_to_check_helper():
    page = render()
    check_no_broken_links(page)


def test_explicit_fail():
    if not ready():
        pytest.fail("service never became ready")


class TestUnittestStyle:
    def test_assert_equal(self):
        self.assertEqual(add(2, 2), 4)

    def test_assert_raises(self):
        with self.assertRaises(KeyError):
            lookup("missing")


# --- Regression cases found by running against real repositories -------------
# Both of these classes of false positive were caught by scanning flask's test
# suite. They must never come back.


def common_object_test(app):
    """A local helper that does the asserting.

    Its name contains no `check`/`verify`/`assert` hint, so only real call-graph
    resolution can see that it asserts. Taken from flask/tests/test_config.py.
    """
    assert app.secret_key == "config"


def test_delegates_to_local_helper():
    app = make_app()
    common_object_test(app)


def indirectly_asserts(app):
    """Asserts only via another local helper — requires transitive resolution."""
    common_object_test(app)


def test_delegates_transitively():
    app = make_app()
    indirectly_asserts(app)


def test_with_nested_route_handler(client):
    # Flask test suites define view functions named `test`. They are nested, so
    # no runner ever collects them — and neither may we.
    @app.route("/test")
    def test():
        return "ok"

    assert client.get("/test").status_code == 200


def test_with_nested_click_command(runner):
    # Same shape with click commands, from flask/tests/test_cli.py.
    @click.command()
    def testcmd():
        click.echo("hi")

    assert runner.invoke(testcmd).exit_code == 0


# --- Regression cases found by scanning sqlalchemy, pydantic and celery ------


def test_uses_nose_style_assertion():
    # SQLAlchemy's suite is built on `eq_`/`is_`/`ne_` rather than bare `assert`.
    # Not knowing these made us report 36% of its 12,716 tests as vacuous.
    eq_(compute(), 3)


@profiling.function_call_count()
def test_decorator_does_the_asserting():
    # SQLAlchemy asserts on call counts from the decorator, not in the body.
    t1.insert().compile()


@pytest.mark.benchmark(group="complete")
def test_benchmark_has_no_assertions(benchmark):
    # pydantic: the `benchmark` fixture measures and checks for regressions.
    # Note that ordinary `pytest.mark.*` labels stay inert — only markers with
    # real behaviour suppress.
    benchmark(validate, payload())


def test_returns_a_value_for_its_decorator():
    # SQLAlchemy's CacheKeySuite calls the returned lambdas and compares them.
    def stmt0():
        return select(qt_table)

    return [stmt0]


def test_decorated_nested_function():
    # The assertion lives on the inner function's decorator.
    @profiling.function_call_count()
    def go():
        t1.update().compile()

    go()


def not_a_test_at_all():
    """Must not be collected as a test in the first place."""
    pass
