"""Mocking a dependency — or mocking to create a precondition — is normal."""

from unittest.mock import patch


@patch("myapp.billing.stripe_client")
def test_charge_card(mock_stripe):
    # Mocks a dependency, not the subject. The real `charge_card` still runs.
    result = charge_card(order)
    assert result.status == "ok"


def test_db(mocker):
    # `db` is below the minimum symbol length: names this short collide by
    # coincidence too often to accuse anyone over.
    mocked = mocker.patch("myapp.db")
    connect()
    mocked.assert_called_once()


def test_unrelated_name(mocker):
    mocker.patch("myapp.billing.charge_card")
    assert reconcile_ledger() == 0


# --- Regression cases found by scanning rich, requests and flask -------------
# Every one of these was a false positive from the first version of this rule,
# which matched on the name alone. They are why the rule now also requires that
# the test asserts on nothing but mocks.


def test_input(monkeypatch, capsys):
    # rich patches `builtins.input` inside `test_input` — same name, entirely
    # different thing — and asserts on real captured output.
    monkeypatch.setattr("builtins.input", fake_input)
    console = Console()
    user_input = console.input(prompt="foo:")
    assert capsys.readouterr().out == "foo:"
    assert user_input == "bar"


def test_idna(mocker):
    # requests patches `requests.help.idna` to *create* the condition under
    # test, then asserts on a real value.
    mocker.patch("requests.help.idna", new=None)
    assert info()["idna"] == {"version": ""}


def test_init_db(monkeypatch):
    # flask's tutorial patches `init_db` to check that the CLI wiring calls it,
    # and asserts on real command output.
    monkeypatch.setattr("flaskr.db.init_db", fake_init_db)
    result = runner.invoke(args=["init-db"])
    assert "Initialized" in result.output
