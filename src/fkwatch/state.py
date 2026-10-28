from __future__ import annotations

import json
from pathlib import Path

from .models import RunReport


def _snapshot(report: RunReport) -> dict[str, dict[str, object]]:
    return {
        result.product.pid: {
            "available": result.available,
            "price": result.price,
            "error": result.error,
        }
        for result in report.results
    }


def load_state(path: Path) -> dict[str, dict[str, object]]:
    if not path.exists():
        return {}
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (ValueError, OSError):
        return {}
    return data if isinstance(data, dict) else {}


def save_state(path: Path, report: RunReport) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(_snapshot(report), indent=2, sort_keys=True), encoding="utf-8")


def is_changed(previous: dict[str, dict[str, object]], report: RunReport) -> bool:
    if not previous:
        return True
    current = _snapshot(report)
    for pid, state in current.items():
        old = previous.get(pid) or {}
        if state["error"] or old.get("error"):
            continue
        if bool(state["available"]) != bool(old.get("available")):
            return True
        if state["price"] != old.get("price"):
            return True
    return False
