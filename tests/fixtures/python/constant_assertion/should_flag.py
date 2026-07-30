"""Every assertion here is on literals."""


def test_asserts_true():
    assert True


def test_asserts_identity():
    assert 1 == 1


def test_asserts_string_equality():
    assert "expected" == "expected"


def test_all_assertions_constant():
    assert not False
    assert "a" != "b"


class TestUnittestStyle:
    def test_assert_equal_constants(self):
        self.assertEqual(3, 3)

    def test_assert_true_literal(self):
        self.assertTrue(True)
