"""A shared base class, in a module no test runner collects.

This is where suites keep their assertion helpers. pyflakes does exactly this,
and resolving helpers one file at a time reported all 436 of its tests as
asserting nothing.
"""


class TestCase:
    def flakes(self, source, *expected):
        actual = check(source)
        assert actual == list(expected)

    def assert_round_trips(self, value):
        assert decode(encode(value)) == value
