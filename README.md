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

The repository includes a local PostgreSQL service definition in [compose.yml](compose.yml).

## Quick Start

1. Install dependencies:

```bash
uv sync
```

2. Start PostgreSQL. One option is the bundled compose file:

```bash
docker compose up -d postgres
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

To inspect all effective runtime configuration values from the current shell or env file, run:

```bash
uv run stock-cache config show
uv run stock-cache --env-file /path/to/.env config show
```

## Running The CLI

The supported entrypoints are:

- `uv run stock-cache ...`
- `uv run python -m cli ...`

Examples below use the console-script form.

To force the CLI to read a specific env file, pass the global option before the subcommand:

```bash
uv run stock-cache --env-file /path/to/.env write --mode full
```

Shell-exported environment variables still override values loaded from `--env-file` or the default `.env`.

## Global Install And Standalone Skills

Install the tool and the standalone skills from this checkout:

```bash
uv run stock-cache install-skill --token YOUR_TUSHARE_TOKEN
```

After install, both runtime forms are supported:

```bash
stock-cache --help
uv tool run stock-cache --help
```

The installed standalone home is:

```text
~/.agents/skills/stock-cache
```

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
| `WRITE_BATCH_SIZE` | No | `500` | maximum rows per repository upsert batch |
| `LOG_LEVEL` | No | `INFO` | logging level |

The example file lives at [`.env.example`](.env.example).

Config precedence is:

1. shell `export`, for example `export TUSHARE_TOKEN=...`
2. `--env-file /path/to/.env`
3. default `.env` in the current working directory
4. field defaults defined in [src/config.py](src/config.py)

`uv run stock-cache config show` prints every configured variable as an `ENV_NAME=value` line using the final resolved runtime value after parsing and precedence rules are applied.

## Database Initialization

Run:

```bash
uv run stock-cache init-db
uv run stock-cache --env-file /path/to/.env init-db
```

This command loads [src/db/schema.sql](src/db/schema.sql), ensures the core tables exist, and prints JSON like:

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
uv run stock-cache --env-file /path/to/.env write --mode full
```

Sync a single stock by `ts_code`:

```bash
uv run stock-cache write --mode single --ts-code 000001.SZ
uv run stock-cache --env-file /path/to/.env write --mode single --ts-code 000001.SZ
```

or resolve a single stock from the cached instruments table by name:

```bash
uv run stock-cache write --mode single --name 平安银行
uv run stock-cache --env-file /path/to/.env write --mode single --name 平安银行
```

Override the default recent trading-day window from the CLI:

```bash
uv run stock-cache write --mode full --lookback-trading-days 30
uv run stock-cache --env-file /path/to/.env write --mode full --lookback-trading-days 30
```

Or sync an absolute trade-date range:

```bash
uv run stock-cache write --mode full \
  --start-date 2026-01-01 \
  --end-date 2026-03-31
uv run stock-cache --env-file /path/to/.env write \
  --mode full \
  --start-date 2026-01-01 \
  --end-date 2026-03-31
```

The CLI accepts two `--mode` values:

- `full`: sync all active instruments for the selected window
- `single`: sync exactly one instrument selected by `--ts-code` or `--name`

During a write run, the CLI prints progress lines to `stderr` so you can see what stage it is in. The final machine-readable job summary is still printed to `stdout`. A successful run looks like:

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

During `write --mode full`, the CLI now fetches one trade date at a time, normalizes that trade date immediately, and persists rows in chunks controlled by `WRITE_BATCH_SIZE`. This keeps write-memory usage bounded by the current trade date payload plus the current repository batch instead of the whole write window.

Write window rules:

- `uv run stock-cache write --mode full` uses `DEFAULT_LOOKBACK_TRADING_DAYS`
- `uv run stock-cache write --mode single --ts-code 000001.SZ` uses the same write window rules, but only for that instrument
- `--ts-code` and `--name` can only be used with `--mode single`
- `--mode single` requires exactly one of `--ts-code` or `--name`
- `--lookback-trading-days` overrides `DEFAULT_LOOKBACK_TRADING_DAYS` for that command only
- `--start-date` and `--end-date` must be provided together
- `--lookback-trading-days` cannot be combined with `--start-date` / `--end-date`

## Cache Stats

Use `stats date-range` to inspect the queryable cached trade-date segments in each table:

```bash
uv run stock-cache stats date-range
```

Response shape:

```json
{
  "data": {
    "daily_market": {
      "min_trade_date": "2026-01-02",
      "max_trade_date": "2026-03-31",
      "continuous_ranges": [
        ["2026-01-02", "2026-01-05", "2026-01-06"],
        ["2026-03-31"]
      ]
    },
    "daily_indicators": {
      "min_trade_date": "2026-01-02",
      "max_trade_date": "2026-03-31",
      "continuous_ranges": [
        ["2026-01-02", "2026-01-05", "2026-01-06"],
        ["2026-03-31"]
      ]
    }
  }
}
```

`continuous_ranges` is a two-dimensional array of actual cached trade dates grouped into continuous trading-date segments. It does not assume the cache is complete between the global minimum and maximum dates.

## Delete Cached Data

Delete one cached trade date:

```bash
uv run stock-cache delete by-date --trade-date 2026-03-31
```

Delete a cached date range:

```bash
uv run stock-cache delete by-date \
  --start-date 2026-01-01 \
  --end-date 2026-01-31
```

Delete response shape:

```json
{
  "query": {
    "start_date": "2026-01-01",
    "end_date": "2026-01-31"
  },
  "data": {
    "daily_market_deleted": 12,
    "daily_indicators_deleted": 9
  },
  "meta": {
    "total_deleted_rows": 21
  }
}
```

## Read Raw Data

Use `read raw` to fetch cached history for one stock and date range:

```bash
uv run stock-cache read raw \
  --ts-code 000001.SZ \
  --start-date 2026-01-01 \
  --end-date 2026-03-30
```

You can also resolve the stock by exact instrument name from the cached `instruments` table:

```bash
uv run stock-cache read raw \
  --name "Ping An Bank" \
  --start-date 2026-01-01 \
  --end-date 2026-03-30
```

Provide exactly one of `--ts-code` or `--name`.

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

`init-db`, `write`, `read raw`, and `read screen` all perform a PostgreSQL reachability check before continuing. If the configured `POSTGRES_DSN` is not reachable, the CLI exits with JSON like:

```json
{
  "status": "error",
  "error": "postgres_unreachable",
  "message": "PostgreSQL is not reachable at configured POSTGRES_DSN."
}
```

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

- [src/cli.py](src/cli.py): CLI entrypoint
- [src/config.py](src/config.py): settings
- [src/db/](src/db): schema and PostgreSQL helpers
- [src/providers/](src/providers): upstream provider integration
- [src/repositories/](src/repositories): persistence layer
- [src/services/](src/services): normalization, retry, status file support
- [src/use_cases/](src/use_cases): application flows for write and read commands
- [tests/](tests): unit and integration tests

For repository-level instructions aimed at code agents, see [AGENTS.md](AGENTS.md).
