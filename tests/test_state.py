from fkwatch.models import CheckResult, Product, RunReport
from fkwatch.state import is_changed

PRODUCT = Product("Space Black", "/oppo-find-x9-space-black-256-gb/p/itma96fe4cab1395", "MOBHGZZ8GH2Z85MY")


def report(*results):
    return RunReport(pincode="412101", address="test", results=list(results))


def test_first_run_is_always_a_change():
    assert is_changed({}, report(CheckResult(PRODUCT, in_stock=False, price=79999.0)))


def test_availability_flip_is_a_change():
    previous = {PRODUCT.pid: {"available": False, "price": 79999.0, "error": None}}
    current = report(CheckResult(PRODUCT, in_stock=True, deliverable=True, available=True, price=79999.0))
    assert is_changed(previous, current)


def test_same_status_and_price_is_not_a_change():
    previous = {PRODUCT.pid: {"available": False, "price": 79999.0, "error": None}}
    current = report(CheckResult(PRODUCT, in_stock=False, price=79999.0))
    assert not is_changed(previous, current)


def test_price_change_is_a_change():
    previous = {PRODUCT.pid: {"available": False, "price": 79999.0, "error": None}}
    current = report(CheckResult(PRODUCT, in_stock=False, price=74999.0))
    assert is_changed(previous, current)


def test_recovering_from_an_error_is_a_change():
    previous = {PRODUCT.pid: {"available": False, "price": None, "error": "NO_PINCODE"}}
    current = report(CheckResult(PRODUCT, in_stock=True, deliverable=True, available=True, price=78978.0))
    assert is_changed(previous, current)


def test_current_error_does_not_spam():
    previous = {PRODUCT.pid: {"available": False, "price": 79999.0, "error": None}}
    current = report(CheckResult(PRODUCT, error="timeout"))
    assert not is_changed(previous, current)
