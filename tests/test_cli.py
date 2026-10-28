from fkwatch.cli import should_notify
from fkwatch.models import CheckResult, Product, RunReport

PRODUCT = Product(name="Space Black", path="/x", pid="ABC")


def report(*results):
    return RunReport(pincode="412101", address="Bengaluru", results=list(results))


def test_always_mode_always_notifies():
    assert should_notify("always", report(CheckResult(product=PRODUCT)), {}) is True


def test_available_mode_skips_when_nothing_available():
    assert should_notify("available", report(CheckResult(product=PRODUCT)), {}) is False


def test_available_mode_notifies_when_one_is_available():
    available = CheckResult(product=PRODUCT, in_stock=True, deliverable=True, available=True)
    assert should_notify("available", report(available), {}) is True


def test_available_mode_skips_when_every_check_failed():
    failed = CheckResult(product=PRODUCT, error="NO_PINCODE")
    assert should_notify("available", report(failed), {}) is False


def test_changes_mode_uses_previous_state():
    unchanged = CheckResult(product=PRODUCT, price=100.0)
    previous = {PRODUCT.pid: {"available": False, "price": 100.0, "error": None}}
    assert should_notify("changes", report(unchanged), previous) is False

    changed = CheckResult(product=PRODUCT, price=90.0)
    assert should_notify("changes", report(changed), previous) is True
