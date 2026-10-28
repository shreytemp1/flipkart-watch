from __future__ import annotations

import html
import logging
from datetime import timezone, timedelta

import requests

from .models import CheckResult, RunReport

LOG = logging.getLogger(__name__)

IST = timezone(timedelta(hours=5, minutes=30))
TELEGRAM_LIMIT = 4096
SAFE_CHUNK = 3800


class NotificationError(RuntimeError):
    pass


def _esc(value: str) -> str:
    return html.escape(value, quote=False)


def _link(result: CheckResult) -> str:
    url = html.escape(result.product.url, quote=True)
    return f'<a href="{url}">Buy now on Flipkart</a>'


def render_message(report: RunReport, device: str) -> str:
    stamp = report.checked_at.astimezone(IST).strftime("%Y-%m-%d %H:%M IST")
    head = (
        f"📍 {_esc(report.address)}\n"
        f"Pincode <b>{_esc(report.pincode)}</b>"
    )

    if report.any_available:
        lines = [
            f"✅ <b>{_esc(device)}</b> — AVAILABLE at {_esc(report.pincode)}",
            head,
            "",
        ]
        for result in report.available:
            lines.append(
                f"• <b>{_esc(result.product.name)}</b> — <b>{result.price_text()}</b>\n"
                f"   {_link(result)}"
            )
        others = [r for r in report.results if not r.available and r.ok]
        if others:
            lines.append("")
            lines.append("<i>Not available:</i>")
            for result in others:
                lines.append(f"• {_esc(result.product.name)} — {_esc(result.reason)} — {result.price_text()}")
    elif not any(result.ok for result in report.results):
        lines = [
            f"⚠️ <b>{_esc(device)}</b> — COULD NOT CHECK at {_esc(report.pincode)}",
            head,
            "",
        ]
    else:
        lines = [
            f"❌ <b>{_esc(device)}</b> — NOT AVAILABLE at {_esc(report.pincode)}",
            head,
            "",
        ]
        for result in report.results:
            if result.ok:
                lines.append(
                    f"• {_esc(result.product.name)} — {_esc(result.reason)} — {result.price_text()}"
                )

    if report.errors:
        lines.append("")
        lines.append("⚠️ <i>Could not check:</i>")
        for result in report.errors:
            lines.append(f"• {_esc(result.product.name)} — {_esc(result.error or 'unknown error')}")

    lines.append("")
    lines.append(f"🕒 {stamp}")
    return "\n".join(lines)


def chunk_message(text: str, limit: int = SAFE_CHUNK) -> list[str]:
    if len(text) <= limit:
        return [text]
    chunks: list[str] = []
    current: list[str] = []
    size = 0
    for line in text.splitlines():
        addition = len(line) + 1
        if current and size + addition > limit:
            chunks.append("\n".join(current))
            current, size = [], 0
        current.append(line)
        size += addition
    if current:
        chunks.append("\n".join(current))
    return chunks


class TelegramNotifier:
    def __init__(self, token: str, chat_id: str, *, api_base: str = "https://api.telegram.org") -> None:
        self.token = token
        self.chat_id = chat_id
        self.api_base = api_base.rstrip("/")

    def send(self, text: str) -> int:
        sent = 0
        for chunk in chunk_message(text):
            response = requests.post(
                f"{self.api_base}/bot{self.token}/sendMessage",
                json={
                    "chat_id": self.chat_id,
                    "text": chunk,
                    "parse_mode": "HTML",
                    "disable_web_page_preview": True,
                },
                timeout=30,
            )
            if response.status_code != 200 or not response.json().get("ok"):
                raise NotificationError(
                    f"Telegram rejected the message: HTTP {response.status_code} {response.text[:200]}"
                )
            sent += 1
        return sent
