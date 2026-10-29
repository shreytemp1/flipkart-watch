from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime, timezone


@dataclass(frozen=True)
class Product:
    name: str
    path: str
    pid: str
    min_coupon: float | None = None

    @property
    def url(self) -> str:
        return f"https://www.flipkart.com{self.path}?pid={self.pid}"


@dataclass
class CheckResult:
    product: Product
    available: bool = False
    in_stock: bool = False
    deliverable: bool = False
    price: float | None = None
    mrp: float | None = None
    title: str | None = None
    unserviceability_reason: str | None = None
    coupon: float | None = None
    error: str | None = None

    @property
    def ok(self) -> bool:
        return self.error is None

    @property
    def coupon_ok(self) -> bool:
        """Whether the listing carries the coupon the user asked for."""
        if self.product.min_coupon is None:
            return True
        return self.coupon is not None and self.coupon >= self.product.min_coupon

    @property
    def qualifies(self) -> bool:
        """Whether this listing should trigger an alert (stock + coupon gate)."""
        return self.available and self.coupon_ok

    @property
    def reason(self) -> str:
        if self.error:
            return self.error
        if self.available:
            if not self.coupon_ok:
                return f"no ₹{self.product.min_coupon:,.0f} coupon"
            return "available"
        if not self.in_stock:
            return "out of stock"
        if not self.deliverable:
            return "not deliverable to this pincode"
        return "unavailable"

    def price_text(self) -> str:
        if self.price is None:
            return "price unknown"
        return f"₹{self.price:,.0f}"


@dataclass
class RunReport:
    pincode: str
    address: str
    results: list[CheckResult] = field(default_factory=list)
    checked_at: datetime = field(default_factory=lambda: datetime.now(timezone.utc))

    @property
    def available(self) -> list[CheckResult]:
        """Listings that are buyable *and* meet their coupon requirement."""
        return [r for r in self.results if r.qualifies]

    @property
    def errors(self) -> list[CheckResult]:
        return [r for r in self.results if r.error]

    @property
    def any_available(self) -> bool:
        return bool(self.available)
