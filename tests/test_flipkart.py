import json
from pathlib import Path

import pytest

from fkwatch.flipkart import FlipkartClient, LOCATION_UPDATE_URL, PAGE_FETCH_URL
from fkwatch.models import Product

FIXTURES = Path(__file__).parent / "fixtures"
PRODUCT = Product(
    name="Space Black",
    path="/oppo-find-x9-space-black-256-gb/p/itma96fe4cab1395",
    pid="MOBHGZZ8GH2Z85MY",
)


class FakeResponse:
    def __init__(self, status_code=200, payload=None):
        self.status_code = status_code
        self._payload = payload
        self.text = json.dumps(payload) if payload is not None else ""

    def json(self):
        if self._payload is None:
            raise ValueError("no json")
        return self._payload


class FakeSession:
    def __init__(self, responses):
        self.headers = {}
        self.calls = []
        self._responses = list(responses)

    def request(self, method, url, **kwargs):
        self.calls.append({"method": method, "url": url, "kwargs": kwargs})
        if not self._responses:
            raise AssertionError(f"unexpected extra request: {method} {url}")
        return self._responses.pop(0)


def payload(name):
    return json.loads((FIXTURES / name).read_text(encoding="utf-8"))


def client(responses, **kwargs):
    sleeps = []
    return (
        FlipkartClient(
            "412101",
            session=FakeSession(responses),
            sleep=sleeps.append,
            **kwargs,
        ),
        sleeps,
    )


def test_happy_path_binds_pincode_and_returns_availability():
    c, _ = client([FakeResponse(200), FakeResponse(200), FakeResponse(200, payload("in_stock.json"))])
    result = c.check(PRODUCT)

    assert result.available is True
    assert result.price == 79999.0
    calls = c.session.calls
    assert [call["method"] for call in calls] == ["POST", "GET", "POST"]
    assert calls[0]["url"] == LOCATION_UPDATE_URL
    assert calls[1]["url"] == PRODUCT.url
    assert calls[2]["url"] == PAGE_FETCH_URL

    location_body = calls[0]["kwargs"]["json"]
    assert location_body["addressInfo"]["pincode"] == "412101"
    assert calls[0]["kwargs"]["headers"]["x-user-agent"].endswith("FKUA/msite/0.0.4/msite/Mobile")

    fetch_body = calls[2]["kwargs"]["json"]
    assert fetch_body["pageUri"] == f"{PRODUCT.path}?pid={PRODUCT.pid}"
    assert fetch_body["locationContext"]["pincode"] == 412101


def test_retries_transient_status():
    c, sleeps = client(
        [
            FakeResponse(200),
            FakeResponse(200),
            FakeResponse(406),
            FakeResponse(200, payload("in_stock.json")),
        ],
        retries=2,
        backoff=0.5,
    )
    result = c.check(PRODUCT)
    assert result.ok and result.available
    assert sleeps == [0.5]


def test_restarts_session_and_retries_when_pincode_not_applied():
    no_pin = payload("out_of_stock.json")
    no_pin["RESPONSE"]["pageData"]["pageContext"]["fdpEventTracking"]["events"]["psi"]["pls"][
        "unserviceabilityReason"
    ] = "NO_PINCODE"

    c, _ = client(
        [
            FakeResponse(200),
            FakeResponse(200),
            FakeResponse(200, no_pin),
            FakeResponse(200),
            FakeResponse(200),
            FakeResponse(200, payload("in_stock.json")),
        ]
    )
    result = c.check(PRODUCT)
    assert result.available is True
    assert sum(1 for call in c.session.calls if call["method"] == "GET") == 2
    posts = [call for call in c.session.calls if call["method"] == "POST"]
    assert sum(1 for call in posts if call["url"] == LOCATION_UPDATE_URL) == 2


def test_missing_payload_restarts_session_and_retries():
    c, _ = client(
        [
            FakeResponse(200),
            FakeResponse(200),
            FakeResponse(200, {"RESPONSE": {"pageData": {}}}),
            FakeResponse(200),
            FakeResponse(200),
            FakeResponse(200, payload("in_stock.json")),
        ]
    )
    result = c.check(PRODUCT)
    assert result.ok and result.available is True


def test_retry_uses_a_fresh_session():
    no_pin = payload("out_of_stock.json")
    no_pin["RESPONSE"]["pageData"]["pageContext"]["fdpEventTracking"]["events"]["psi"]["pls"][
        "unserviceabilityReason"
    ] = "NO_PINCODE"
    first = FakeSession([FakeResponse(200), FakeResponse(200), FakeResponse(200, no_pin)])
    second = FakeSession([FakeResponse(200), FakeResponse(200), FakeResponse(200, payload("in_stock.json"))])
    sessions = [first, second]

    c = FlipkartClient("412101", session_factory=lambda: sessions.pop(0), sleep=lambda _: None)
    result = c.check(PRODUCT)

    assert result.available is True
    assert c.session is second
    assert len(first.calls) == 3
    assert len(second.calls) == 3


def test_pincode_still_not_applied_returns_error():
    no_pin = payload("out_of_stock.json")
    no_pin["RESPONSE"]["pageData"]["pageContext"]["fdpEventTracking"]["events"]["psi"]["pls"][
        "unserviceabilityReason"
    ] = "NO_PINCODE"

    c, _ = client(
        [
            FakeResponse(200),
            FakeResponse(200),
            FakeResponse(200, no_pin),
            FakeResponse(200),
            FakeResponse(200),
            FakeResponse(200, no_pin),
        ]
    )
    result = c.check(PRODUCT)
    assert result.available is False
    assert result.error is not None
    assert "NO_PINCODE" in result.error


def test_location_update_with_wrong_pincode_is_an_error():
    wrong = {"RESPONSE": {"currentPincode": "400001"}}
    c, _ = client([FakeResponse(200, wrong)])
    result = c.check(PRODUCT)
    assert result.available is False
    assert result.error is not None
    assert "400001" in result.error


def test_prime_failure_returns_error_not_false_negative():
    c, _ = client([FakeResponse(500), FakeResponse(500), FakeResponse(500)], retries=1)
    result = c.check(PRODUCT)
    assert result.error is not None
    assert result.ok is False


def test_invalid_json_is_an_error():
    class BadJson(FakeResponse):
        def json(self):
            raise ValueError("bad")

    c, _ = client([FakeResponse(200), FakeResponse(200), BadJson(200)])
    result = c.check(PRODUCT)
    assert result.error is not None


def test_one_session_handles_multiple_products():
    second = Product(
        name="Velvet Red",
        path="/oppo-find-x9-velvet-red-256-gb/p/itma96fe4cab1395",
        pid="MOBHG8Y56JNQGGTX",
    )
    c, _ = client(
        [
            FakeResponse(200),
            FakeResponse(200),
            FakeResponse(200, payload("out_of_stock.json")),
            FakeResponse(200, payload("in_stock.json")),
        ]
    )
    results = c.check_all([PRODUCT, second])
    assert len(results) == 2
    assert results[0].available is False
    assert results[1].available is True
    gets = [call for call in c.session.calls if call["method"] == "GET"]
    assert len(gets) == 1


def test_rejects_bad_pincode():
    with pytest.raises(ValueError):
        FlipkartClient("12345")
