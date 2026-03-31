---
name: stock-cache-write
description: Use when working in this repository and needing to populate or refresh the local PostgreSQL cache from Tushare, initialize the schema before syncing, inspect the latest write status file, or rerun the supported write flow with the project's CLI.
---

# Stock Cache Write

## Overview

Use the repository's Typer CLI from the repo root to push A-share data from Tushare into PostgreSQL.
Prefer `uv run stock-cache ...`; keep the CLI thin and do not invent alternate entry scripts.

## Preconditions

Run commands from the repository root.

Before writing data, make sure:

```bash
uv sync
docker compose up -d postgres
cp .env.example .env
```

Set at least these variables in `.env`:

- `POSTGRES_DSN`
- `TUSHARE_TOKEN`

Initialize the schema before the first write against a fresh database:

```bash
uv run stock-cache init-db
```

## Write Workflow

Run a normal cache refresh with:

```bash
uv run stock-cache write --mode full
```

To sync one stock by `ts_code`:

```bash
uv run stock-cache write --mode single --ts-code 000001.SZ
```

To resolve one stock from the cached `instruments` table by exact name:

```bash
uv run stock-cache write --mode single --name 平安银行
```

To override the default recent trading-day window for one run:

```bash
uv run stock-cache write --mode full --lookback-trading-days 30
```

To sync an absolute trade-date range for the full universe:

```bash
uv run stock-cache write --mode full \
  --start-date 2026-01-01 \
  --end-date 2026-03-31
```

Range rules:

- `uv run stock-cache write --mode full` still uses `DEFAULT_LOOKBACK_TRADING_DAYS`
- `uv run stock-cache write --mode single --ts-code 000001.SZ` uses the same date-window rules, but only for that instrument
- `--ts-code` and `--name` can only be used with `--mode single`
- `--mode single` requires exactly one of `--ts-code` or `--name`
- `--lookback-trading-days` overrides the default lookback for that command only
- `--start-date` and `--end-date` must be passed together
- `--lookback-trading-days` cannot be combined with `--start-date` / `--end-date`

## Delete Cached Data By Date

Delete one cached trade date:

```bash
uv run stock-cache delete by-date --trade-date 2026-03-31
```

Delete an absolute cached range:

```bash
uv run stock-cache delete by-date \
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

## Common Command Sequence

Use this order when setting up or refreshing a local cache:

```bash
uv sync
docker compose up -d postgres
uv run stock-cache init-db
uv run stock-cache write --mode full
```

## Troubleshooting

If `init-db` or `write` fails immediately, check:

- PostgreSQL is reachable through `POSTGRES_DSN`
- `.env` exists in the repo root
- `TUSHARE_TOKEN` is set to a valid token

If you need to inspect the supported top-level commands, run:

```bash
uv run stock-cache --help
uv run stock-cache write --help
uv run stock-cache delete --help
```
