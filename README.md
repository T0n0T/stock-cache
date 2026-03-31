# Stock Cache

`stock-cache` is a Typer-based CLI that pulls recent A-share market data from Tushare, stores normalized rows in PostgreSQL, and exposes JSON read commands for raw history and simple screening queries.

## What The Project Does

- fetches the stock universe and recent trade dates from Tushare
- bulk-syncs daily market, basic, moneyflow, adjustment, suspension, limit, and indicator payloads by trade date
- persists normalized rows into PostgreSQL
- records job-run summaries and writes a fixed status file for the latest write run
- reads cached rows back as JSON for downstream scripts or manual inspection

## Requirements

- Python `3.13`
- `uv`
- PostgreSQL
- a valid `TUSHARE_TOKEN`

The repository includes a local PostgreSQL service definition in [config.yml](/home/pi/Documents/agents/stock-cache/config.yml).

## Quick Start

1. Install dependencies:

```bash
uv sync
```

2. Start PostgreSQL. One option is the bundled compose file:

```bash
docker compose -f config.yml up -d postgres
```

3. Create your environment file:

```bash
cp .env.example .env
```

4. Update `.env` with at least:

- `POSTGRES_DSN`
- `TUSHARE_TOKEN`

5. Initialize the database schema:

```bash
uv run stock-cache init-db
```

6. Run an initial write:

```bash
uv run stock-cache write --mode full
```

## Running The CLI

The supported entrypoints are:

- `uv run stock-cache ...`
- `uv run python -m cli ...`

Examples below use the console-script form.

## Environment Variables

| Variable | Required | Default | Purpose |
| --- | --- | --- | --- |
| `POSTGRES_DSN` | Yes | none | PostgreSQL connection string |
| `TUSHARE_TOKEN` | Yes | none | Tushare API token |
| `MAX_CONCURRENCY` | No | `20` | reserved concurrency setting |
| `MAX_RETRIES` | No | `3` | retry count for retryable provider failures |
| `RETRY_BASE_DELAY` | No | `1.0` | initial retry delay in seconds |
| `RETRY_BACKOFF_FACTOR` | No | `2.0` | exponential backoff multiplier |
| `RETRY_JITTER` | No | `0.2` | random retry jitter in seconds |
| `REQUEST_TIMEOUT_SECONDS` | No | `20` | provider request timeout |
| `DEFAULT_LOOKBACK_TRADING_DAYS` | No | `90` | write window size in recent trade dates |
| `STATUS_FILE_PATH` | No | `.runtime/last-write-status.txt` | last write job summary file |
| `ALLOW_INDICATOR_BACKFILL_ON_READ` | No | `true` | read-time indicator backfill policy flag |
| `ENABLE_TUSHARE_INDICATORS` | No | `true` | provider indicator toggle |
| `ENABLE_LOCAL_INDICATOR_FALLBACK` | No | `true` | allow local indicator fallback logic |
| `WRITE_BATCH_SIZE` | No | `500` | reserved write batching setting |
| `LOG_LEVEL` | No | `INFO` | logging level |

The example file lives at [`.env.example`](/home/pi/Documents/agents/stock-cache/.env.example).

## Database Initialization

Run:

```bash
uv run stock-cache init-db
```

This command loads [src/db/schema.sql](/home/pi/Documents/agents/stock-cache/src/db/schema.sql), ensures the core tables exist, and prints JSON like:

```json
{
  "status": "ok",
  "created_tables": ["daily_indicators", "daily_market", "instruments", "job_runs"],
  "already_present": [],
  "missing": []
}
```

The core tables are:

- `instruments`
- `daily_market`
- `daily_indicators`
- `job_runs`

## Write Workflow

Run a sync job with:

```bash
uv run stock-cache write --mode full
```

or:

```bash
uv run stock-cache write --mode failed-only
```

The CLI currently accepts a `--mode` option and returns a JSON job summary. A successful run looks like:

```json
{
  "job_id": "20260331T120000Z",
  "status": "success",
  "started_at": "2026-03-31T12:00:00+00:00",
  "finished_at": "2026-03-31T12:00:01+00:00",
  "total_symbols": 1,
  "success_symbols": ["000001.SZ"],
  "failed_symbols": {}
}
```

Each write run also overwrites the status file at `STATUS_FILE_PATH`. The file contains a human-readable summary with counts plus successful and failed symbols.

## Read Raw Data

Use `read raw` to fetch cached history for one stock and date range:

```bash
uv run stock-cache read raw \
  --ts-code 000001.SZ \
  --start-date 2026-01-01 \
  --end-date 2026-03-30
```

Response shape:

```json
{
  "query": {
    "ts_code": "000001.SZ",
    "start_date": "2026-01-01",
    "end_date": "2026-03-30"
  },
  "data": {
    "market": [],
    "indicators": []
  },
  "meta": {
    "row_count_market": 0,
    "row_count_indicators": 0
  }
}
```

`market` and `indicators` are serialized from the PostgreSQL cache, with date values emitted as ISO strings.

## Read Screened Data

Use `read screen` to query the cached dataset by trade date and filter thresholds:

```bash
uv run stock-cache read screen \
  --trade-date 2026-03-30 \
  --pct-chg-gte 5 \
  --turnover-rate-gte 3 \
  --macd-gte 0
```

Available filters:

- `--pct-chg-gte`
- `--turnover-rate-gte`
- `--total-mv-gte`
- `--total-mv-lte`
- `--macd-gte`
- `--kdj-j-gte`

Response shape:

```json
{
  "query": {
    "trade_date": "2026-03-30",
    "filters": {
      "pct_chg_gte": 5.0,
      "turnover_rate_gte": 3.0,
      "macd_gte": 0.0
    }
  },
  "data": [
    {
      "ts_code": "300001.SZ",
      "trade_date": "2026-03-30",
      "pct_chg": 5.0,
      "turnover_rate": 3.0,
      "macd": 0.0
    }
  ],
  "meta": {
    "matched": 1
  }
}
```

## Development Workflow

Useful commands:

```bash
uv run stock-cache --help
uv run pytest
uv run pytest tests/test_cli.py tests/test_config.py
```

When working locally, prefer:

1. update or add focused tests for the changed behavior
2. run targeted pytest commands first
3. run broader validation once the touched area is stable

## Repository Layout

- [src/cli.py](/home/pi/Documents/agents/stock-cache/src/cli.py): CLI entrypoint
- [src/config.py](/home/pi/Documents/agents/stock-cache/src/config.py): settings
- [src/db/](/home/pi/Documents/agents/stock-cache/src/db): schema and PostgreSQL helpers
- [src/providers/](/home/pi/Documents/agents/stock-cache/src/providers): upstream provider integration
- [src/repositories/](/home/pi/Documents/agents/stock-cache/src/repositories): persistence layer
- [src/services/](/home/pi/Documents/agents/stock-cache/src/services): normalization, retry, status file support
- [src/use_cases/](/home/pi/Documents/agents/stock-cache/src/use_cases): application flows for write and read commands
- [tests/](/home/pi/Documents/agents/stock-cache/tests): unit and integration tests

For repository-level instructions aimed at code agents, see [AGENTS.md](/home/pi/Documents/agents/stock-cache/AGENTS.md).
