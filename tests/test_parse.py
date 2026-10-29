import json
from pathlib import Path

import pytest

from fkwatch.flipkart import FlipkartError, PincodeNotApplied, best_coupon, parse_page_json
from fkwatch.models import Product

FIXTURES = Path(__file__).parent / "fixtures"
PRODUCT = Product(
    name="Space Black",
    path="/oppo-find-x9-space-black-256-gb/p/itma96fe4cab1395",
    pid="MOBHGZZ8GH2Z85MY",
)
X9S = Product(
    name="X9s Lavender Sky",
    path="/oppo-find-x9s-lavender-sky-256-gb/p/itmeca45f29bf010",
    pid="MOBHN7ZW6SBDHZWR",
    min_coupon=10000,
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


def test_coupon_amount_is_parsed_from_offer():
    result = parse_page_json(load("coupon_offer.json"), PRODUCT)
    assert result.coupon == 10000.0
    assert result.available is True
    assert result.qualifies is True


def test_accessory_coupon_is_not_mistaken_for_a_phone_coupon():
    payload = load("coupon_offer.json")
    # Keep only the "Unlock Oppo coupon • ₹1499 off" node.
    payload["RESPONSE"]["slots"] = payload["RESPONSE"]["slots"][1:]
    assert best_coupon(payload) is None
    assert parse_page_json(payload, PRODUCT).coupon is None


def test_x9s_requires_a_10000_coupon():
    result = parse_page_json(load("coupon_offer.json"), X9S)
    assert result.qualifies is True
    assert result.reason == "available"


def test_x9s_without_coupon_does_not_qualify():
    payload = load("coupon_offer.json")
    payload["RESPONSE"]["slots"] = payload["RESPONSE"]["slots"][1:]
    result = parse_page_json(payload, X9S)
    assert result.available is True
    assert result.coupon is None
    assert result.qualifies is False
    assert result.reason == "no ₹10,000 coupon"


def test_x9s_with_smaller_coupon_does_not_qualify():
    payload = load("coupon_offer.json")
    offer = payload["RESPONSE"]["slots"][0]["widget"]["data"]["offers"][0]["value"]["offerSummariesRC"][0]["value"]
    offer["discountedPriceText"] = "₹9,000 off"
    offer["offerSubTitleRC"]["value"]["contentList"][0]["contentValue"] = "Save ₹9,000 extra on 1 item(s)"
    result = parse_page_json(payload, X9S)
    assert result.coupon == 9000.0
    assert result.qualifies is False
    assert result.reason == "no ₹10,000 coupon"


def test_percentage_coupon_is_ignored():
    payload = load("coupon_offer.json")
    offer = payload["RESPONSE"]["slots"][0]["widget"]["data"]["offers"][0]["value"]["offerSummariesRC"][0]["value"]
    offer["discountedPriceText"] = "10% off"
    offer["offerSubTitleRC"]["value"]["contentList"][0]["contentValue"] = "10% off on 1 item(s)"
    assert best_coupon(payload) is None


def test_x9_listing_ignores_coupon_gate():
    result = parse_page_json(load("coupon_offer.json"), PRODUCT)
    assert result.product.min_coupon is None
    assert result.qualifies == result.available
