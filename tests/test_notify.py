from datetime import datetime, timezone

import pytest

from fkwatch import notify
from fkwatch.models import CheckResult, Product, RunReport
from fkwatch.notify import NotificationError, TelegramNotifier, chunk_message, render_message

PRODUCTS = [
    Product("Space Black", "/oppo-find-x9-space-black-256-gb/p/itma96fe4cab1395", "MOBHGZZ8GH2Z85MY"),
    Product("Velvet Red", "/oppo-find-x9-velvet-red-256-gb/p/itma96fe4cab1395", "MOBHG8Y56JNQGGTX"),
]


def make_report(results):
    return RunReport(
        pincode="412101",
        address="KK Marks Building A & B, Pimpri-Chinchwad, Maharashtra",
        results=results,
        checked_at=datetime(2026, 10, 28, 14, 15, tzinfo=timezone.utc),
    )


def test_not_available_message():
    report = make_report(
        [
            CheckResult(PRODUCTS[0], in_stock=False, deliverable=False, price=79999.0),
            CheckResult(PRODUCTS[1], in_stock=False, deliverable=False, price=79999.0),
        ]
    )
    text = render_message(report, "OPPO Find X9 12+256")
    assert "NOT AVAILABLE at 412101" in text
    assert "KK Marks Building A &amp; B" in text
    assert "Space Black — out of stock — ₹79,999" in text
    assert "19:45 IST" in text
    assert "Buy now" not in text


def test_available_message_includes_price_and_link():
    report = make_report(
        [
            CheckResult(PRODUCTS[0], in_stock=True, deliverable=True, available=True, price=79999.0),
            CheckResult(PRODUCTS[1], in_stock=False, deliverable=False, price=79999.0),
        ]
    )
    text = render_message(report, "OPPO Find X9 12+256")
    assert "AVAILABLE at 412101" in text
    assert "₹79,999" in text
    assert 'href="https://www.flipkart.com/oppo-find-x9-space-black-256-gb/p/itma96fe4cab1395?pid=MOBHGZZ8GH2Z85MY"' in text
    assert "Not available:" in text
    assert "Velvet Red — out of stock" in text


def test_partial_errors_are_surfaced():
    report = make_report(
        [
            CheckResult(PRODUCTS[0], in_stock=False, deliverable=False, price=79999.0),
            CheckResult(PRODUCTS[1], error="request failed after 3 attempts"),
        ]
    )
    text = render_message(report, "OPPO Find X9 12+256")
    assert "Could not check" in text
    assert "request failed after 3 attempts" in text


def test_all_errors_does_not_claim_unavailable():
    report = make_report(
        [
            CheckResult(PRODUCTS[0], error="NO_PINCODE"),
            CheckResult(PRODUCTS[1], error="timeout"),
        ]
    )
    text = render_message(report, "OPPO Find X9 12+256")
    assert "COULD NOT CHECK" in text
    assert "NOT AVAILABLE" not in text
    assert "NO_PINCODE" in text


def test_chunking_preserves_every_line():
    lines = [f"line {i} " + "x" * 80 for i in range(100)]
    text = "\n".join(lines)
    chunks = chunk_message(text, limit=500)
    assert len(chunks) > 1
    assert all(len(chunk) <= 500 for chunk in chunks)
    assert "\n".join(chunks).splitlines() == lines


def test_telegram_send_posts_expected_payload(monkeypatch):
    calls = []

    class Response:
        status_code = 200

        def json(self):
            return {"ok": True}

    def fake_post(url, json, timeout):
        calls.append((url, json))
        return Response()

    monkeypatch.setattr(notify.requests, "post", fake_post)
    sent = TelegramNotifier("TOKEN", "42", api_base="https://api.telegram.org").send("hello")
    assert sent == 1
    url, body = calls[0]
    assert url == "https://api.telegram.org/botTOKEN/sendMessage"
    assert body["chat_id"] == "42"
    assert body["parse_mode"] == "HTML"


def test_telegram_error_raises(monkeypatch):
    class Response:
        status_code = 400
        text = "bad request"

        def json(self):
            return {"ok": False}

    monkeypatch.setattr(notify.requests, "post", lambda *a, **k: Response())
    with pytest.raises(NotificationError):
        TelegramNotifier("TOKEN", "42").send("hello")
