"""None of these may be flagged.

Covers each assertion dialect we claim to understand, plus every way the
assertion can live somewhere other than the test body.
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


def common_object_test(app):
    """Asserts, but nothing in the name says so. Straight out of flask."""
    assert app.secret_key == "config"


def test_delegates_to_local_helper():
    app = make_app()
    common_object_test(app)


def indirectly_asserts(app):
    common_object_test(app)


def test_delegates_transitively():
    app = make_app()
    indirectly_asserts(app)


def test_with_nested_route_handler(client):
    # flask names view functions `test`. Nested, so no runner collects them.
    @app.route("/test")
    def test():
        return "ok"

    assert client.get("/test").status_code == 200


def test_with_nested_click_command(runner):
    @click.command()
    def testcmd():
        click.echo("hi")

    assert runner.invoke(testcmd).exit_code == 0


def test_uses_nose_style_assertion():
    # Some suites use nothing but these.
    eq_(compute(), 3)


@profiling.function_call_count()
def test_decorator_does_the_asserting():
    # The decorator checks the call count.
    t1.insert().compile()


@pytest.mark.benchmark(group="complete")
def test_benchmark_has_no_assertions(benchmark):
    # The fixture measures and checks. Plain pytest.mark labels don't count.
    benchmark(validate, payload())


def test_returns_a_value_for_its_decorator():
    # Something else calls these and compares the results.
    def stmt0():
        return select(qt_table)

    return [stmt0]


def test_decorated_nested_function():
    @profiling.function_call_count()
    def go():
        t1.update().compile()

    go()


def not_a_test_at_all():
    """Shouldn't be collected in the first place."""
    pass
