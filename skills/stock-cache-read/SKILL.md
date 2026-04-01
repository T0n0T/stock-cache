---
name: stock-cache-read
description: Use when reading cached stock history, latest trade dates, or filtered trade-date snapshots from the standalone stock-cache PostgreSQL cache.
---

# Stock Cache Read

## Overview

Use the globally installed `stock-cache` CLI to read cached PostgreSQL data back as JSON.
Prefer read commands only after the database has been initialized and populated by the standalone write flow. The shared standalone home is:

```text
~/.agents/skills/stock-cache
```

## Preconditions

Operate from `~/.agents/skills/stock-cache` so the copied `compose.yml` and generated `.env` are in scope:

```bash
cd ~/.agents/skills/stock-cache
```

Prefer explicitly exporting `.env` before each CLI read instead of assuming the process auto-loads it:

```bash
set -a; source .env
```

Make sure `POSTGRES_DSN` is set, PostgreSQL is running, and the cache has already been populated via `stock-cache write --mode full`.

If the cache is empty, read commands will still return JSON, but `data` may be empty.

When using this skill, check the PostgreSQL cache layer first. A typical local startup command is:

```bash
docker compose up -d postgres
```

If PostgreSQL is unreachable, the CLI now exits with JSON like:

```json
{
  "status": "error",
  "error": "postgres_unreachable",
  "message": "PostgreSQL is not reachable at configured POSTGRES_DSN."
}
```

If Docker reports the container is healthy but the CLI still returns `postgres_unreachable`, treat that as an environment/access problem first:

- Re-run with explicit `.env` export
- In sandboxed environments, retry the read command with escalated permissions before assuming the cache is empty or broken

## Read One Stock

Use `read raw` for one symbol and date range. Provide exactly one of `--ts-code` or `--name`:

```bash
stock-cache read raw \
  --ts-code 000001.SZ \
  --start-date 2026-01-01 \
  --end-date 2026-03-30
```

You can also resolve the stock from the cached `instruments` table by exact name:

```bash
stock-cache read raw \
  --name "Ping An Bank" \
  --start-date 2026-01-01 \
  --end-date 2026-03-30
```

Prefer `YYYY-MM-DD` input dates to match the documented CLI usage.

The JSON payload contains:

- `query.ts_code`, `query.start_date`, `query.end_date`
- `data.market`
- `data.indicators`
- `meta.row_count_market`
- `meta.row_count_indicators`

## Fast Path For One Value

If the task only needs the latest cached close for one stock, do not inspect the full JSON manually.

1. Read the latest cached trade date:

```bash
set -a; source .env; stock-cache stats date-range | jq -r '.data.daily_market.max_trade_date'
```

2. Query that single date and extract only the fields needed:

```bash
set -a; source .env; stock-cache read raw \
  --name "华特气体" \
  --start-date 2026-03-31 \
  --end-date 2026-03-31 | jq '{ts_code: .query.ts_code, latest_market: .data.market[-1] | {trade_date, close}}'
```

This avoids returning a large `market`/`indicators` history blob when the user only asked for one price.

## Screen The Cached Universe

Use `read screen` for one trade date plus optional thresholds via the installed CLI:

```bash
stock-cache read screen \
  --trade-date 2026-03-30 \
  --pct-chg-gte 5 \
  --turnover-rate-gte 3 \
  --macd-gte 0
```

Supported filter flags:

- `--pct-chg-gte`
- `--turnover-rate-gte`
- `--total-mv-gte`
- `--total-mv-lte`
- `--macd-gte`
- `--kdj-j-gte`

Returned rows are JSON objects with fields such as:

- `ts_code`
- `trade_date`
- `pct_chg`
- `turnover_rate`
- `total_mv`
- `macd`
- `kdj_j`

## Inspect Cached Date Segments

Use `stats date-range` to inspect the actual queryable trade-date segments already present in the cache via the standalone CLI:

```bash
stock-cache stats date-range
```

The payload is keyed by table name and includes:

- `min_trade_date`
- `max_trade_date`
- `continuous_ranges`

`continuous_ranges` is a two-dimensional array of actual cached trade dates grouped into continuous trading-date segments. This is meant to surface cache gaps instead of assuming the stored trading dates are complete.

## Reading Guidance

Use `read raw` when the task is about one stock's stored history.
Use `read screen` when the task is about filtering the cached market snapshot for one trade date.
Use `stats date-range` first when the user asks for "latest", "recent", or "most recent" and the exact cached trade date matters.
Use `jq` to project only the requested fields before replying when the CLI returns a large JSON payload.

Consume stdout as JSON rather than scraping text output.

## Limits

- These commands read from the local cache; they are not a replacement for the write flow.
- Do not assume missing rows will be backfilled during reads. In the current code, reads should be treated as cache-first lookups.

## Helpful Checks

Inspect supported commands with:

```bash
stock-cache read --help
stock-cache stats --help
stock-cache read raw --help
stock-cache read screen --help
```
