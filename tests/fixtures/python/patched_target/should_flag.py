"""Tests that mock their own subject and then only ever check the mock.

Both conditions have to hold. Mocking your subject while still asserting on a
real value is legitimate and lives in `should_not_flag.py`.
"""

from unittest.mock import Mock, patch


@patch("myapp.billing.charge_card")
def test_charge_card(mock_charge):
    charge_card(order)
    mock_charge.assert_called_once()


def test_send_email(mocker):
    mocked = mocker.patch("myapp.mail.send_email")
    send_email("a@b.c")
    mocked.assert_called_once_with("a@b.c")


def test_sync_users(monkeypatch):
    tracker = Mock()
    monkeypatch.setattr(worker, "sync_users", tracker)
    sync_users()
    tracker.assert_called_once()
