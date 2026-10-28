import json
from pathlib import Path

import pytest

from fkwatch.flipkart import FlipkartError, PincodeNotApplied, parse_page_json
from fkwatch.models import Product

FIXTURES = Path(__file__).parent / "fixtures"
PRODUCT = Product(
    name="Space Black",
    path="/oppo-find-x9-space-black-256-gb/p/itma96fe4cab1395",
    pid="MOBHGZZ8GH2Z85MY",
)


def load(name: str) -> dict:
    return json.loads((FIXTURES / name).read_text(encoding="utf-8"))


def test_out_of_stock_is_not_available():
    result = parse_page_json(load("out_of_stock.json"), PRODUCT)
    assert result.ok
    assert result.in_stock is False
    assert result.deliverable is False
    assert result.available is False
    assert result.price == 79999.0
    assert result.reason == "out of stock"
    assert result.title == "OPPO Find X9 (256 GB Storage, 12 GB RAM)"


def test_in_stock_and_serviceable_is_available():
    result = parse_page_json(load("in_stock.json"), PRODUCT)
    assert result.available is True
    assert result.in_stock is True
    assert result.deliverable is True
    assert result.price_text() == "₹79,999"
    assert result.reason == "available"


def test_in_stock_but_not_serviceable_is_not_available():
    payload = load("in_stock.json")
    pls = payload["RESPONSE"]["pageData"]["pageContext"]["fdpEventTracking"]["events"]["psi"]["pls"]
    pls["isServiceable"] = False
    pls["serviceable"] = False
    result = parse_page_json(payload, PRODUCT)
    assert result.in_stock is True
    assert result.available is False
    assert result.reason == "not deliverable to this pincode"


def test_no_pincode_raises():
    payload = load("out_of_stock.json")
    pls = payload["RESPONSE"]["pageData"]["pageContext"]["fdpEventTracking"]["events"]["psi"]["pls"]
    pls["unserviceabilityReason"] = "NO_PINCODE"
    with pytest.raises(PincodeNotApplied):
        parse_page_json(payload, PRODUCT)


def test_missing_psi_raises():
    with pytest.raises(FlipkartError):
        parse_page_json({"RESPONSE": {"pageData": {}}}, PRODUCT)
