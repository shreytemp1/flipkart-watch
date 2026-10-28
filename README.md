# flipkart-watch

Watches the four Flipkart listings of the **OPPO Find X9 12 GB + 256 GB** and reports, every 5 minutes via
GitHub Actions, whether any of them is in stock **and deliverable to pincode 412101**. Sends a Telegram message
with the direct link and price when something is available.

- No browser, no API key, no account. Plain HTTPS.
- Every 5 minutes at best (GitHub's cron floor and delay apply).
- If Flipkart drops the pincode context (or returns an incomplete page), the watcher restarts the session and
  retries once before reporting an error.

Read [REQUIREMENTS.md](REQUIREMENTS.md) for scope and [PLAN.md](PLAN.md) for how the Flipkart API was reverse
engineered.

## Watched links

| Colour | Link |
|---|---|
| Space Black | https://www.flipkart.com/oppo-find-x9-space-black-256-gb/p/itma96fe4cab1395?pid=MOBHGZZ8GH2Z85MY |
| Titanium Grey | https://www.flipkart.com/oppo-find-x9-titanium-grey-256-gb/p/itma96fe4cab1395?pid=MOBHGZZ8SAMGGJHR |
| Titanium Gray | https://www.flipkart.com/oppo-find-x9-titanium-gray-256-gb/p/itma96fe4cab1395?pid=MOBHKZVXT93DXHMP |
| Velvet Red | https://www.flipkart.com/oppo-find-x9-velvet-red-256-gb/p/itma96fe4cab1395?pid=MOBHG8Y56JNQGGTX |

## Local usage

```bash
python3 -m venv .venv && . .venv/bin/activate
pip install -e ".[dev]"
pytest

# show what would be sent, without Telegram
python -m fkwatch --dry-run

# real run
export TELEGRAM_BOT_TOKEN="123:abc"
export TELEGRAM_CHAT_ID="123456789"
python -m fkwatch
```

Exit codes: `0` all checked, `1` some listings could not be checked, `2` everything failed, `3` Telegram failed.

## Telegram setup

1. Open `@BotFather` in Telegram, send `/newbot`, follow prompts, copy the token.
2. Send any message to your new bot.
3. Open `https://api.telegram.org/bot<TOKEN>/getUpdates` and copy `result[0].message.chat.id`.
4. In the GitHub repo: Settings → Secrets and variables → Actions → add
   `TELEGRAM_BOT_TOKEN` and `TELEGRAM_CHAT_ID`.

## Deploy to GitHub Actions

```bash
git remote add origin https://github.com/<you>/flipkart-watch.git
git push -u origin main
```

Then add `TELEGRAM_BOT_TOKEN` and `TELEGRAM_CHAT_ID` under **Settings → Secrets and variables → Actions**, and
run the workflow once from the Actions tab ("flipkart-watch" → Run workflow) to verify.

Keep the repo **public** for unlimited Actions minutes; at a 5-minute cadence a private repo exceeds the free
2,000 minutes/month. A public repo exposes the address line in `config.yaml` (pincode-level serviceability does
not depend on it).

## Configuration

`config.yaml` holds pincode, address, listings and the notification mode.

- `notify.mode: always` (default) — one message per run, as requested.
- `notify.mode: changes` — only when availability or price changes. Recommended if 288 messages/day is too
  many.

## Important limits

- GitHub cron cannot run more often than every 5 minutes and is often delayed; it is not a real-time alert.
- 288 runs/day needs a **public** repo (unlimited Actions minutes). Private repos only get 2,000 min/month.
- Scheduled workflows are disabled after 60 days without repo activity — push any commit to re-enable.
- Flipkart's internal API is undocumented. If it changes, runs report `⚠️ Could not check` and exit non-zero
  instead of lying about stock.
