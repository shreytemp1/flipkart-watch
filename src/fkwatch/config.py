from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

import yaml

from .models import Product


class ConfigError(ValueError):
    pass


@dataclass(frozen=True)
class NotifyConfig:
    mode: str = "always"


@dataclass(frozen=True)
class HttpConfig:
    timeout_seconds: float = 30.0
    retries: int = 3
    backoff_seconds: float = 2.0


@dataclass(frozen=True)
class TelegramConfig:
    api_base: str = "https://api.telegram.org"


@dataclass(frozen=True)
class Config:
    pincode: str
    address: str
    device: str
    products: list[Product]
    notify: NotifyConfig
    http: HttpConfig
    telegram: TelegramConfig

    @classmethod
    def load(cls, path: str | Path) -> "Config":
        raw = yaml.safe_load(Path(path).read_text(encoding="utf-8"))
        if not isinstance(raw, dict):
            raise ConfigError(f"{path}: expected a mapping at the top level")

        pincode = str(raw.get("pincode", "")).strip()
        if not (pincode.isdigit() and len(pincode) == 6):
            raise ConfigError(f"{path}: pincode must be a 6-digit string, got {pincode!r}")

        addr = raw.get("address") or {}
        if not isinstance(addr, dict):
            raise ConfigError(f"{path}: address must be a mapping")
        address = ", ".join(
            str(addr[key]).strip()
            for key in ("line1", "city", "state")
            if str(addr.get(key, "")).strip()
        )

        raw_products = raw.get("products") or []
        if not raw_products:
            raise ConfigError(f"{path}: at least one product is required")
        products: list[Product] = []
        for i, item in enumerate(raw_products):
            if not isinstance(item, dict):
                raise ConfigError(f"{path}: products[{i}] must be a mapping")
            path_value = str(item.get("path", "")).strip()
            pid = str(item.get("pid", "")).strip()
            if not path_value or not pid:
                raise ConfigError(f"{path}: products[{i}] needs both path and pid")
            products.append(
                Product(
                    name=str(item.get("name", f"listing {i + 1}")).strip(),
                    path=path_value if path_value.startswith("/") else "/" + path_value,
                    pid=pid,
                )
            )

        notify_raw = raw.get("notify") or {}
        mode = str(notify_raw.get("mode", "always")).strip().lower()
        if mode not in {"always", "changes", "available"}:
            raise ConfigError(
                f"{path}: notify.mode must be 'always', 'changes' or 'available', got {mode!r}"
            )

        http_raw = raw.get("http") or {}
        telegram_raw = raw.get("telegram") or {}

        return cls(
            pincode=pincode,
            address=address,
            device=str(raw.get("device", "OPPO Find X9 12+256")).strip(),
            products=products,
            notify=NotifyConfig(mode=mode),
            http=HttpConfig(
                timeout_seconds=float(http_raw.get("timeout_seconds", 30)),
                retries=max(1, int(http_raw.get("retries", 3))),
                backoff_seconds=float(http_raw.get("backoff_seconds", 2)),
            ),
            telegram=TelegramConfig(
                api_base=str(telegram_raw.get("api_base", "https://api.telegram.org")).rstrip("/")
            ),
        )
