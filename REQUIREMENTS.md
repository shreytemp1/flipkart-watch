# Requirements — Flipkart Find X9 Stock Watcher

Status: agreed baseline, v1.

## 1. Goal

Watch every Flipkart listing of the **OPPO Find X9 base model, 12 GB RAM + 256 GB storage**, check whether it is
buyable and deliverable to **pincode 412101** (KK Marks Building A & B, Pimpri-Chinchwad, Maharashtra), and report
the result to Telegram. Runs unattended from GitHub Actions on an effective 3-minute cadence
(two checks per 6-minute workflow run).

## 2. Watched listings

Verified live on 2026-10-28. All four are the same product (`itma96fe4cab1395`) in different colours.

| # | Colour | Product ID | URL |
|---|--------|-----------|-----|
| 1 | Space Black | `MOBHGZZ8GH2Z85MY` | https://www.flipkart.com/oppo-find-x9-space-black-256-gb/p/itma96fe4cab1395?pid=MOBHGZZ8GH2Z85MY |
| 2 | Titanium Grey | `MOBHGZZ8SAMGGJHR` | https://www.flipkart.com/oppo-find-x9-titanium-grey-256-gb/p/itma96fe4cab1395?pid=MOBHGZZ8SAMGGJHR |
| 3 | Titanium Gray (2nd listing) | `MOBHKZVXT93DXHMP` | https://www.flipkart.com/oppo-find-x9-titanium-gray-256-gb/p/itma96fe4cab1395?pid=MOBHKZVXT93DXHMP |
| 4 | Velvet Red | `MOBHG8Y56JNQGGTX` | https://www.flipkart.com/oppo-find-x9-velvet-red-256-gb/p/itma96fe4cab1395?pid=MOBHG8Y56JNQGGTX |

Explicitly **out of scope**: Find X9 Pro, Find X9 Ultra, Find X9s, Find X10, 512 GB / 16 GB variants, Flipkart
refurbished / third-party marketplaces (Amazon, OPPO store).

## 3. Functional requirements

- **FR-1** For each watched listing, determine stock state (`IN_STOCK` / `OUT_OF_STOCK`).
- **FR-2** For each watched listing, determine pincode serviceability for 412101.
- **FR-3** A listing counts as **AVAILABLE** only when it is in stock *and* serviceable at the pincode.
- **FR-4** Capture the current selling price (`finalPrice`) and MRP for every listing.
- **FR-5** Send exactly one consolidated Telegram message per run containing:
  - if anything is available: the available listing(s) with **price and direct link**;
  - if nothing is available: an explicit "not available" message listing each colour and the reason
    (out of stock / not deliverable).
- **FR-6** Notification mode is configurable:
  - `always` (default): send every run, as requested;
  - `changes`: send only when availability flips or price changes;
  - `available`: send only when at least one listing is in stock and deliverable.
- **FR-7** Failures (network, HTTP 4xx/5xx, missing fields, pincode not applied) must never be reported as
  "not available" silently; they are reported as an **error** message and the run exits non-zero so GitHub
  flags it.
- **FR-8** The run must survive transient failures: 3 attempts per listing with backoff.
- **FR-9** Emit a machine-readable JSON result so runs are auditable from Actions artifacts.

## 4. Non-functional requirements

- **NFR-1** No headless browser. Plain HTTPS requests only (fast, cheap, works on `ubuntu-latest`).
- **NFR-2** No secrets in the repo. Telegram credentials only via GitHub Secrets / environment variables.
- **NFR-3** Total runtime per run < 60 s for 4 listings.
- **NFR-4** Deterministic, unit-testable parsing (fixtures captured from the real API).
- **NFR-5** Idempotent: concurrent runs must not overlap (Actions `concurrency` group).

## 5. Constraints and known limitations

- **C-1 GitHub cron floor.** `schedule` minimum is 5 minutes, but GitHub may delay or drop runs under load.
  The 3-minute cadence is implemented as two checks (t=0 and t=3 min) inside a 6-minute run. This is a platform
  workaround, not a true 3-minute cron; gaps can exceed 3 minutes when GitHub delays a run.
- **C-2 3-minute polling is 480 messages/day** in `always` mode. Switch `notify.mode: changes` to reduce noise.
- **C-3 Private-repo minutes.** 240 jobs/day, each holding a runner for ~4 minutes (sleep + two checks), is
  roughly 28,800 Actions minutes/month — far above the 2,000 free minutes for private repos. Keep the repo
  **public** (unlimited minutes).
- **C-4 Scheduled workflows are auto-disabled after 60 days of repo inactivity.** Any commit re-enables them.
- **C-5 IP reputation.** Flipkart may rate-limit or block cloud IPs. On block the run reports an error rather
  than a false "unavailable".
- **C-6 Address precision.** Flipkart serviceability is pincode-level; the building/flat text does not change the
  result. It is included in the message for clarity only.
- **C-7 Anti-bot changes.** The API is undocumented. Parsing is defensive and fixture-tested; a Flipkart change
  surfaces as an error, not a wrong answer.

## 6. Acceptance criteria

1. `cargo test` passes offline.
2. `cargo run -- --dry-run` prints the exact Telegram message for the current live state of all 4 links.
3. A live run against pincode 412101 correctly reports the current truth (today: all four out of stock).
4. With valid credentials, one message arrives in Telegram per run.
5. GitHub Actions workflow runs on schedule and via manual dispatch.

## 7. Credentials required from the owner

| Variable | Where | How to obtain |
|---|---|---|
| `TELEGRAM_BOT_TOKEN` | GitHub Secret | `@BotFather` → `/newbot` → token |
| `TELEGRAM_CHAT_ID` | GitHub Secret | message the bot, open `https://api.telegram.org/bot<TOKEN>/getUpdates` → `chat.id` |

Optional: a GitHub repository/remote to push to (`gh` CLI is not installed in this environment).
