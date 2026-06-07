---
name: stock-cache-write
description: Use when populating, refreshing, or repairing the standalone stock-cache PostgreSQL cache from Tushare, including schema setup, targeted rewrites, and post-write verification.
---

# Stock Cache Write

## Overview

Use the globally installed `stock-cache` CLI to push A-share data from Tushare into PostgreSQL. The commands align with the standalone runtime installed under `~/.agents/skills/stock-cache`.

## Preconditions

Operate from the installed standalone home so the copied `compose.yml` and generated `.env` are in scope:

```bash
cd ~/.agents/skills/stock-cache
```

Before writing data, make sure you have a valid `POSTGRES_DSN` and `TUSHARE_TOKEN` configured in your environment. These values need not live in this repository once the standalone CLI is installed.

Prefer `--env-file .env` on every CLI command instead of assuming the shell already exported the standalone runtime variables:

```bash
stock-cache --env-file .env config show
```

The installed standalone runtime also includes an editable default index list at:

```text
~/.agents/skills/stock-cache/.runtime/default-indexes.csv
```

`write --mode full` uses this CSV to decide which indexes to sync. Edit the file when you want to add, remove, or disable indexes without changing code.

If you only want the index phase, use:

```bash
stock-cache --env-file .env write --mode indexes --start-date 2025-01-01 --end-date 2026-05-04
```

Start PostgreSQL as needed, for example with the bundled compose definition in:

```bash
docker compose up -d postgres
```

Use `config show` as a lightweight preflight before a write when environment problems are plausible:

```bash
stock-cache --env-file .env config show
```

Initialize the schema before the first write against a fresh database with the installed CLI:

```bash
stock-cache --env-file .env init-db
```

## Write Workflow

Run a normal cache refresh with the standalone CLI:

```bash
stock-cache --env-file .env write --mode full
```

That full write now syncs both:

- stock market data for the active A-share universe
- index daily data from `.runtime/default-indexes.csv`

To sync one stock by `ts_code`:

```bash
stock-cache --env-file .env write --mode single --ts-code 000001.SZ
```

To resolve one stock from the cached `instruments` table by exact name:

```bash
stock-cache --env-file .env write --mode single --name 平安银行
```

To override the default recent trading-day window for one run:

```bash
stock-cache --env-file .env write --mode full --lookback-trading-days 30
```

To refresh only the most recent trading day, use:

```bash
stock-cache --env-file .env write --mode full --lookback-trading-days 1
```

To sync an absolute trade-date range for the full universe:

```bash
stock-cache --env-file .env write --mode full \
  --start-date 2026-01-01 \
  --end-date 2026-03-31
```

To backfill a larger absolute date range with bounded trade-date concurrency:

```bash
stock-cache --env-file .env write --mode full \
  --start-date 2026-01-01 \
  --end-date 2026-03-31 \
  --max-concurrency 4
```

Range rules:

- `stock-cache write --mode full` still uses `DEFAULT_LOOKBACK_TRADING_DAYS`
- `stock-cache write --mode single --ts-code 000001.SZ` uses the same date-window rules, but only for that instrument
- `stock-cache write --mode indexes` uses the same date-window rules, but only for configured indexes
- `--ts-code` and `--name` can only be used with `--mode single`
- `--mode single` requires exactly one of `--ts-code` or `--name`
- `--lookback-trading-days` overrides the default lookback for that command only
- `--start-date` and `--end-date` must be passed together
- `--lookback-trading-days` cannot be combined with `--start-date` / `--end-date`
- `--max-concurrency` overrides `MAX_CONCURRENCY` for the full-mode stock trade-date phase only

## Fastest Safe Workflow

Use this path when the goal is "run a write with the least ambiguity":

1. Confirm the active DSN and token source:

```bash
stock-cache --env-file .env config show
```

2. Make sure PostgreSQL is up:

```bash
docker compose up -d postgres
```

3. Initialize schema if the database may be fresh:

```bash
stock-cache --env-file .env init-db
```

4. Run the narrowest write that satisfies the task:

- one symbol: `stock-cache --env-file .env write --mode single --ts-code 000001.SZ`
- latest one trading day: `stock-cache --env-file .env write --mode full --lookback-trading-days 1`
- one date window: `stock-cache --env-file .env write --mode full --start-date 2026-03-01 --end-date 2026-03-31`
- larger backfill window: `stock-cache --env-file .env write --mode full --start-date 2026-01-01 --end-date 2026-03-31 --max-concurrency 4`
- general refresh: `stock-cache --env-file .env write --mode full`

Prefer a targeted `single` or explicit date-range rewrite over a blind full refresh when the task is to repair one symbol or one known bad window.

## Delete Cached Data By Date

Delete one cached trade date:

```bash
stock-cache --env-file .env delete by-date --trade-date 2026-03-31
```

Delete an absolute cached range:

```bash
stock-cache --env-file .env delete by-date \
  --start-date 2026-01-01 \
  --end-date 2026-01-31
```

The command deletes matching rows from both `daily_market` and `daily_indicators` and returns JSON row counts. Use this when you need to trim or re-sync a known cached date window.

## What To Read After A Write

The command streams progress lines to `stderr`, then prints the final JSON summary to `stdout`. Expect fields like:

- `job_id`
- `status`
- `started_at`
- `finished_at`
- `total_symbols`
- `success_symbols`
- `failed_symbols`

Each run also overwrites the fixed status file at `STATUS_FILE_PATH`.
Default path:

```text
.runtime/last-write-status.txt
```

Read that file when you need a human-readable list of successes and failures.

For machine verification, prefer an immediate cache read after the write instead of relying only on the text status file.

## Fast Verification After Write

Use one of these short checks right after a write:

1. Confirm the cache exposes the expected latest trade date:

```bash
stock-cache --env-file .env stats date-range | jq -r '.data.daily_market.max_trade_date'
```

2. Confirm one repaired symbol now has rows in the intended window:

```bash
stock-cache --env-file .env read raw \
  --ts-code 000001.SZ \
  --start-date 2026-03-01 \
  --end-date 2026-03-31 | jq '{ts_code: .query.ts_code, row_count_market: .meta.row_count_market, row_count_indicators: .meta.row_count_indicators}'
```

This gives a faster success signal than manually scanning the full write summary or status text.

## Common Command Sequence

Use this order when setting up or refreshing a local cache with the installed CLI:

```bash
cd ~/.agents/skills/stock-cache
docker compose up -d postgres
stock-cache --env-file .env config show
stock-cache --env-file .env init-db
stock-cache --env-file .env write --mode full
```

## Troubleshooting

If `init-db` or `write` fails immediately, check:

- PostgreSQL is reachable through `POSTGRES_DSN`
- `~/.agents/skills/stock-cache/.env` exists for the installed standalone home
- `TUSHARE_TOKEN` is set to a valid token
- `stock-cache --env-file .env config show` prints the DSN/token pair you expect

If Docker reports PostgreSQL healthy but `init-db`, `write`, or `read` still fails to connect, treat sandbox/network access restrictions as a possible cause before assuming the cache runtime is misconfigured.

If you need to inspect the supported top-level commands, run:

```bash
stock-cache --help
stock-cache write --help
stock-cache delete --help
```
