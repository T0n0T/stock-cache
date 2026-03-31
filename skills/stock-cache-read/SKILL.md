---
name: stock-cache-read
description: Use when working in this repository and needing to read cached stock data from the local PostgreSQL database as JSON, fetch one symbol's raw history by date range, or screen the cached universe for a trade date with threshold filters.
---

# Stock Cache Read

## Overview

Use the repository's CLI to read cached PostgreSQL data back as JSON.
Prefer read commands only after the database has been initialized and populated by the write flow.

## Preconditions

Run commands from the repository root.

Make sure:

- `.env` contains a valid `POSTGRES_DSN`
- PostgreSQL is running before any read attempt
- the cache has already been populated with `uv run stock-cache write --mode full`

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

## Read One Stock

Use `read raw` for one symbol and date range. Provide exactly one of `--ts-code` or `--name`:

```bash
uv run stock-cache read raw \
  --ts-code 000001.SZ \
  --start-date 2026-01-01 \
  --end-date 2026-03-30
```

You can also resolve the stock from the cached `instruments` table by exact name:

```bash
uv run stock-cache read raw \
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

## Screen The Cached Universe

Use `read screen` for one trade date plus optional thresholds:

```bash
uv run stock-cache read screen \
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

Use `stats date-range` to inspect the actual queryable trade-date segments already present in the cache:

```bash
uv run stock-cache stats date-range
```

The payload is keyed by table name and includes:

- `min_trade_date`
- `max_trade_date`
- `continuous_ranges`

`continuous_ranges` is a two-dimensional array of actual cached trade dates grouped into continuous trading-date segments. This is meant to surface cache gaps instead of assuming the stored trading dates are complete.

## Reading Guidance

Use `read raw` when the task is about one stock's stored history.
Use `read screen` when the task is about filtering the cached market snapshot for one trade date.

Consume stdout as JSON rather than scraping text output.

## Limits

- These commands read from the local cache; they are not a replacement for the write flow.
- Do not assume missing rows will be backfilled during reads. In the current code, reads should be treated as cache-first lookups.

## Helpful Checks

Inspect supported commands with:

```bash
uv run stock-cache read --help
uv run stock-cache stats --help
uv run stock-cache read raw --help
uv run stock-cache read screen --help
```
