"""Mocking a dependency, or mocking to set up a condition, is routine."""

from unittest.mock import patch


@patch("myapp.billing.stripe_client")
def test_charge_card(mock_stripe):
    # Mocks a dependency. The real charge_card still runs.
    result = charge_card(order)
    assert result.status == "ok"


def test_db(mocker):
    # Too short a name to match on; two-letter symbols collide by accident.
    mocked = mocker.patch("myapp.db")
    connect()
    mocked.assert_called_once()


def test_unrelated_name(mocker):
    mocker.patch("myapp.billing.charge_card")
    assert reconcile_ledger() == 0


def test_input(monkeypatch, capsys):
    # rich patches builtins.input here. Same name, different thing, and the
    # assertions are on real captured output.
    monkeypatch.setattr("builtins.input", fake_input)
    console = Console()
    user_input = console.input(prompt="foo:")
    assert capsys.readouterr().out == "foo:"
    assert user_input == "bar"


def test_idna(mocker):
    # requests patches this to create the condition it's testing.
    mocker.patch("requests.help.idna", new=None)
    assert info()["idna"] == {"version": ""}


def test_init_db(monkeypatch):
    # Checks that the CLI wiring calls init_db, and asserts on real output.
    monkeypatch.setattr("flaskr.db.init_db", fake_init_db)
    result = runner.invoke(args=["init-db"])
    assert "Initialized" in result.output
