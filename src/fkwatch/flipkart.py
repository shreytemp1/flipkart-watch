from __future__ import annotations

import logging
import time
from typing import Any, Callable, Iterable

import requests

from .models import CheckResult, Product

LOG = logging.getLogger(__name__)

BASE = "https://www.flipkart.com"
USER_AGENT = (
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"
)
X_USER_AGENT = f"{USER_AGENT} FKUA/msite/0.0.4/msite/Mobile"

PAGE_FETCH_URL = f"{BASE}/api/4/page/fetch?cacheFirst=false"
LOCATION_UPDATE_URL = f"{BASE}/api/4/location/update"


class FlipkartError(RuntimeError):
    """A check could not be completed reliably."""


class PincodeNotApplied(FlipkartError):
    """The server answered without binding the requested pincode."""


class MissingPayload(FlipkartError):
    """The page response did not contain the product payload."""


def _psi(payload: dict[str, Any]) -> dict[str, Any]:
    response = payload.get("RESPONSE") if isinstance(payload, dict) else None
    page = (response or payload or {}).get("pageData") or {}
    context = page.get("pageContext") or {}
    tracking = context.get("fdpEventTracking") or {}
    events = tracking.get("events") or {}
    psi = events.get("psi")
    if not isinstance(psi, dict) or "pls" not in psi:
        raise MissingPayload("response did not contain the expected 'psi.pls' payload")
    return psi


def _title(payload: dict[str, Any]) -> str | None:
    response = payload.get("RESPONSE") if isinstance(payload, dict) else None
    page = (response or payload or {}).get("pageData") or {}
    seo = ((page.get("seoData") or {}).get("seo") or {}).get("title")
    if isinstance(seo, str) and seo.strip():
        return seo.split(" Online at Best Price")[0].strip()
    return None


def parse_page_json(payload: dict[str, Any], product: Product) -> CheckResult:
    psi = _psi(payload)
    pls = psi.get("pls") or {}
    ppd = psi.get("ppd") or {}

    reason = pls.get("unserviceabilityReason")
    if reason == "NO_PINCODE":
        raise PincodeNotApplied("Flipkart answered with NO_PINCODE (location binding failed)")

    in_stock = pls.get("availabilityStatus") == "IN_STOCK" or pls.get("isAvailable") is True
    deliverable = pls.get("isServiceable") is True
    price = ppd.get("finalPrice")
    mrp = ppd.get("mrp")

    return CheckResult(
        product=product,
        in_stock=bool(in_stock),
        deliverable=bool(deliverable),
        available=bool(in_stock and deliverable),
        price=float(price) if isinstance(price, (int, float)) else None,
        mrp=float(mrp) if isinstance(mrp, (int, float)) else None,
        title=_title(payload),
        unserviceability_reason=reason,
    )


class FlipkartClient:
    def __init__(
        self,
        pincode: str,
        *,
        timeout: float = 30.0,
        retries: int = 3,
        backoff: float = 2.0,
        session: requests.Session | None = None,
        session_factory: Callable[[], requests.Session] | None = None,
        sleep: Callable[[float], None] = time.sleep,
    ) -> None:
        if not (pincode.isdigit() and len(pincode) == 6):
            raise ValueError(f"invalid pincode: {pincode!r}")
        self.pincode = pincode
        self.timeout = timeout
        self.retries = max(1, retries)
        self.backoff = backoff
        self._session_factory = session_factory
        self.session = session or (session_factory or self._new_session)()
        self._primed = False
        self._bound = False
        self._sleep = sleep

    @staticmethod
    def _new_session() -> requests.Session:
        session = requests.Session()
        session.headers.update(
            {
                "User-Agent": USER_AGENT,
                "Accept-Language": "en-IN,en;q=0.9",
                "Accept": "*/*",
            }
        )
        return session

    def _request(self, method: str, url: str, **kwargs: Any) -> requests.Response:
        last: Exception | None = None
        for attempt in range(1, self.retries + 1):
            try:
                response = self.session.request(method, url, timeout=self.timeout, **kwargs)
                if response.status_code in (406, 429, 500, 502, 503, 504):
                    raise FlipkartError(f"HTTP {response.status_code} from {url}")
                return response
            except (requests.RequestException, FlipkartError) as exc:
                last = exc
                if attempt == self.retries:
                    break
                LOG.warning("attempt %d/%d failed: %s", attempt, self.retries, exc)
                self._sleep(self.backoff * attempt)
        raise FlipkartError(f"request failed after {self.retries} attempts: {last}") from last

    def reset_session(self) -> None:
        """Throw away session cookies and force a fresh prime before the next fetch."""
        cookies = getattr(self.session, "cookies", None)
        if cookies is not None:
            try:
                cookies.clear()
            except (AttributeError, TypeError):
                pass
        if self._session_factory is not None:
            try:
                self.session.close()
            except (AttributeError, TypeError):
                pass
            self.session = self._session_factory()
        self._primed = False
        self._bound = False

    def prime(self, product: Product) -> None:
        if self._primed:
            return
        self._request("GET", product.url)
        self._primed = True
        self.bind_pincode()

    def bind_pincode(self) -> None:
        body = {
            "geoLocation": {"latitude": "18.6742528", "longitude": "73.7301331"},
            "addressInfo": {
                "addressLine1": "Vikas Nagar",
                "city": "Pimpri-Chinchwad",
                "state": "Maharashtra",
                "pincode": self.pincode,
            },
            "redirectionUrl": "",
            "marketplace": "FLIPKART",
        }
        response = self._request(
            "POST",
            LOCATION_UPDATE_URL,
            json=body,
            headers={
                "x-user-agent": X_USER_AGENT,
                "content-type": "application/json",
                "referer": f"{BASE}/",
            },
        )
        if response.status_code >= 400:
            raise FlipkartError(f"location update failed: HTTP {response.status_code}")
        try:
            payload = response.json()
        except ValueError:
            payload = None
        if isinstance(payload, dict):
            current = (payload.get("RESPONSE") or {}).get("currentPincode")
            if current is not None and str(current) != self.pincode:
                raise PincodeNotApplied(
                    f"location update bound pincode {current!r}, expected {self.pincode!r}"
                )
        self._bound = True

    def fetch(self, product: Product) -> dict[str, Any]:
        body = {
            "pageUri": f"{product.path}?pid={product.pid}",
            "pageContext": {"trackingContext": {}, "networkSpeed": 10000},
            "locationContext": {"pincode": int(self.pincode), "changed": False},
        }
        response = self._request(
            "POST",
            PAGE_FETCH_URL,
            json=body,
            headers={
                "x-user-agent": X_USER_AGENT,
                "content-type": "application/json",
                "referer": f"{BASE}/",
                "origin": BASE,
            },
        )
        try:
            return response.json()
        except ValueError as exc:
            raise FlipkartError(f"non-JSON response (HTTP {response.status_code})") from exc

    def check(self, product: Product) -> CheckResult:
        try:
            self.prime(product)
        except FlipkartError as exc:
            return CheckResult(product=product, error=str(exc))

        last_error: FlipkartError | None = None
        for attempt in range(2):
            try:
                return parse_page_json(self.fetch(product), product)
            except (PincodeNotApplied, MissingPayload) as exc:
                last_error = exc
                if attempt:
                    break
                LOG.info(
                    "%s for %s, restarting session and retrying once", exc, product.pid
                )
                self.reset_session()
                try:
                    self.prime(product)
                except FlipkartError as prime_exc:
                    return CheckResult(product=product, error=str(prime_exc))
            except FlipkartError as exc:
                return CheckResult(product=product, error=str(exc))
        return CheckResult(product=product, error=str(last_error))

    def check_all(self, products: Iterable[Product]) -> list[CheckResult]:
        results: list[CheckResult] = []
        for product in products:
            LOG.info("checking %s (%s)", product.name, product.pid)
            results.append(self.check(product))
        return results
