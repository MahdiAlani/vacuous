"""None of these may be flagged: the asserting happens in harness.py."""

from harness import TestCase


class TestImports(TestCase):
    def test_unused_import(self):
        self.flakes("import os", UnusedImport)

    def test_used_import(self):
        self.flakes("import os; os.getcwd()")

    def test_round_trip(self):
        self.assert_round_trips({"a": 1})


def test_still_caught_when_nothing_asserts():
    # The helper resolution must not turn into a blanket amnesty.
    build_thing()
