# Plan — Flipkart Find X9 Stock Watcher

## 1. What the site actually exposes (reverse-engineered, 2026-10-28)

Flipkart's logged-out UI no longer offers a plain pincode box; it opens a "Select delivery address" panel that
requires login. However the product page itself is fetched through an internal page API that accepts a pincode
context. Three plain HTTPS calls reproduce it with no browser:

1. `GET  https://www.flipkart.com/<product-path>?pid=<pid>`
   → establishes session cookies (`T`, `SN`, `at`, `ud`, `bm_*`).
2. `POST https://www.flipkart.com/api/4/location/update`
   body: `{"geoLocation":{...},"addressInfo":{"...","pincode":"412101"},"redirectionUrl":"","marketplace":"FLIPKART"}`
   → binds the pincode to the session.
3. `POST https://www.flipkart.com/api/4/page/fetch?cacheFirst=false`
   body: `{"pageUri":"/<path>?pid=<pid>","pageContext":{...},"locationContext":{"pincode":412101,"changed":false}}`
   headers must include `x-user-agent: <UA> FKUA/msite/0.0.4/msite/Mobile`
   → full page JSON (~650 KB).

Authoritative fields live at `RESPONSE.pageData.pageContext.fdpEventTracking.events.psi`:

| Field | Meaning |
|---|---|
| `psi.pls.availabilityStatus` | `IN_STOCK` / `OUT_OF_STOCK` |
| `psi.pls.isAvailable` | buyable |
| `psi.pls.isServiceable` | deliverable to the bound pincode |
| `psi.pls.unserviceabilityReason` | `null` when fine, else `NotAvailable`, `NO_PINCODE`, … |
| `psi.ppd.finalPrice` / `mrp` | selling price / MRP |
| `psi.fsn` | product id echo |
| `pageData.seoData.seo.title` | product title |

`NO_PINCODE` means the pincode did not bind → treated as an error and retried, never as "unavailable".

Verified live:

- OPPO Find X9 12+256 (all four colours): `OUT_OF_STOCK`, `isServiceable=false` → not available.
- Samsung M36 5G (control): `IN_STOCK`, `isServiceable=true` → available. Parser distinguishes both.

## 2. Architecture

```
config.yaml ──┐
              ├─> fkwatch::cli ─> FlipkartClient ─> Flipkart page API
env secrets ──┘         │                │
                        │                └─> parse psi -> CheckResult
                        ├─> notifier (Telegram)  [skipped in --dry-run]
                        ├─> state store (changes mode)
                        └─> result.json (Actions artifact)
```

Module map:

| File | Responsibility |
|---|---|
| `src/config.rs` | load/validate `config.yaml` |
| `src/models.rs` | `Product`, `CheckResult`, `RunReport` |
| `src/flipkart.rs` | session priming, pincode binding, page fetch, retries, parsing |
| `src/notify.rs` | message rendering (HTML, ≤4096 chars) + Telegram send |
| `src/state.rs` | last-seen status file for `changes` mode |
| `src/cli.rs` | orchestration, exit codes, JSON output |
| `src/main.rs` | binary entry point |

## 3. Availability rule

```
available = isAvailable AND isServiceable
```

If `pls` is missing entirely → status `error` ("could not determine"). Never map unknown to "not available".

## 4. Notification format

Available:
```
✅ OPPO Find X9 12+256 — AVAILABLE at 412101
• Space Black — ₹79,999 — https://www.flipkart.com/...
Checked 2026-10-28 19:45 IST
```

Not available:
```
❌ OPPO Find X9 12+256 — NOT AVAILABLE at 412101 (KK Marks Building A & B)
• Space Black — out of stock — ₹79,999
• Titanium Grey — out of stock — ₹79,999
Wallet/price shown as last seen.
Checked 2026-10-28 19:45 IST
```

Errors are a separate `⚠️` message listing the failing listing and the reason.

## 5. Delivery pipeline

`.github/workflows/watch.yml`
- `repository_dispatch` type `flipkart-stock-check` (fired by cron-job.org every ~2 minutes) + `workflow_dispatch`
- `concurrency: {group: flipkart-watch, cancel-in-progress: false}`
- `timeout-minutes: 10`
- builds the Rust binary (`cargo build --release --locked`) and runs `./target/release/fkwatch`
- uploads `result.json` as an artifact
- secrets: `TELEGRAM_BOT_TOKEN`, `TELEGRAM_CHAT_ID`
- Actions cache restores/saves `.state/`

## 6. Build phases

1. **Docs + config** — this file, `REQUIREMENTS.md`, `config.yaml`, `README.md`.
2. **Client** — 3-step Flipkart flow with retries and defensive parsing.
3. **Notifier** — Telegram HTML messages, chunking, dry-run rendering.
4. **CLI + state** — modes, exit codes, JSON artifact.
5. **Tests** — fixture-based parser tests, mocked HTTP flow tests, notifier tests.
6. **Live verification** — run against the 4 real links, confirm the current truth.
7. **CI** — workflow file; owner adds secrets and pushes.

## 7. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Flipkart changes the internal API | All parsing defensive; failures become `error`, exit non-zero; fixtures pin the contract |
| Cloud IP blocked (403/406) | Browser-like headers, retry/backoff, error report instead of false negative |
| Pincode silently not applied (`NO_PINCODE`) or payload missing (`NO_PSI`) | Treat as retryable, discard cookies, re-prime with a fresh session, retry once; if still failing, mark `error` (never "not available") |
| GitHub delays/drops scheduled runs | Documented; the 3-minute cadence is two checks per 6-minute run; manual dispatch available |
| 480 messages/day too noisy | `notify.mode: changes` |
| Price flips between sellers | Price comes from the same `psi.ppd` payload as availability |

## 8. Open items for the owner

- Telegram bot token + chat id (blocking for live notifications only).
- Where to push the repo (no `gh` CLI installed here).
