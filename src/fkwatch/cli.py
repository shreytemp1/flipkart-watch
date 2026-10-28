from __future__ import annotations

import argparse
import json
import logging
import os
import sys
from dataclasses import asdict
from pathlib import Path

from .config import Config, ConfigError
from .flipkart import FlipkartClient, FlipkartError
from .models import RunReport
from .notify import NotificationError, TelegramNotifier, render_message
from .state import is_changed, load_state, save_state

LOG = logging.getLogger("fkwatch")


def build_report(config: Config) -> RunReport:
    client = FlipkartClient(
        config.pincode,
        timeout=config.http.timeout_seconds,
        retries=config.http.retries,
        backoff=config.http.backoff_seconds,
    )
    results = client.check_all(config.products)
    return RunReport(pincode=config.pincode, address=config.address, results=results)


def report_to_json(report: RunReport) -> dict[str, object]:
    return {
        "pincode": report.pincode,
        "address": report.address,
        "checked_at": report.checked_at.isoformat(),
        "any_available": report.any_available,
        "results": [
            {
                **asdict(result),
                "product": {
                    "name": result.product.name,
                    "pid": result.product.pid,
                    "url": result.product.url,
                },
            }
            for result in report.results
        ],
    }


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="fkwatch",
        description="Check Flipkart Find X9 listings for stock and pincode delivery, then notify Telegram.",
    )
    parser.add_argument("--config", default="config.yaml", help="path to config.yaml (default: config.yaml)")
    parser.add_argument("--dry-run", action="store_true", help="print the message instead of sending it")
    parser.add_argument("--json-out", default="result.json", help="write a JSON report here (default: result.json)")
    parser.add_argument("--no-json", action="store_true", help="do not write the JSON report")
    parser.add_argument("--state", default=".state/fkwatch.json", help="state file for notify.mode=changes")
    parser.add_argument("--verbose", "-v", action="store_true", help="debug logging")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    logging.basicConfig(
        level=logging.DEBUG if args.verbose else logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )

    try:
        config = Config.load(args.config)
    except (ConfigError, OSError) as exc:
        LOG.error("configuration error: %s", exc)
        return 2

    try:
        report = build_report(config)
    except FlipkartError as exc:
        LOG.error("could not complete any check: %s", exc)
        return 2

    if not args.no_json:
        Path(args.json_out).write_text(json.dumps(report_to_json(report), indent=2), encoding="utf-8")

    message = render_message(report, config.device)

    state_path = Path(args.state)
    previous = load_state(state_path)

    should_send = True
    if config.notify.mode == "changes" and not is_changed(previous, report):
        should_send = False
        LOG.info("no availability/price change; skipping notification")

    if args.dry_run:
        print(message)
        print()
        print(f"[dry-run] would send {len(message)} chars; mode={config.notify.mode}; "
              f"available={report.any_available}; errors={len(report.errors)}")
    elif not should_send:
        LOG.info("notification suppressed by notify.mode=changes")
    else:
        token = os.environ.get("TELEGRAM_BOT_TOKEN", "").strip()
        chat_id = os.environ.get("TELEGRAM_CHAT_ID", "").strip()
        if not token or not chat_id:
            LOG.error("TELEGRAM_BOT_TOKEN and TELEGRAM_CHAT_ID must be set")
            return 3
        try:
            chunks = TelegramNotifier(token, chat_id, api_base=config.telegram.api_base).send(message)
            LOG.info("sent %d Telegram message(s)", chunks)
        except NotificationError as exc:
            LOG.error("telegram delivery failed: %s", exc)
            return 3

    if not args.no_json and not args.dry_run:
        save_state(state_path, report)

    if report.errors and len(report.errors) == len(report.results):
        LOG.error("every listing failed to check")
        return 2
    if report.errors:
        LOG.warning("%d of %d listings could not be checked", len(report.errors), len(report.results))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
