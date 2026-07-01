---
name: stock-cache
description: Maintain the stock-cache project through its Rust CLI. Use when Codex needs to initialize the database, inspect runtime status, validate contract gates, show config, or run cache/calc/full writes for this repository.
---

# Stock Cache

Use this skill to operate the `stock-cache` CLI safely and consistently.

## Start Here

Read the command shape first:

- `init-db` creates the PostgreSQL schema.
- `status` reports source, classification, calc, and warmup coverage.
- `contract-check stock-select` runs the DB-native gate before downstream release work.
- `config show` prints the effective runtime configuration.
- `write --mode cache|calc|full` performs data maintenance.

Prefer the CLI over direct SQL for maintenance tasks unless the user explicitly asks for a query.

## Runtime Inputs

The CLI reads shell environment variables and optional `--env-file` values. Shell values override the env file.

Required for database-backed commands:

- `POSTGRES_DSN`
- `TUSHARE_TOKEN` for `config show` and any `write` mode except `calc`

Useful optional settings:

- `DEFAULT_LOOKBACK_TRADING_DAYS`
- `MAX_CONCURRENCY`
- `CACHE_WARMUP_CONCURRENCY`
- `CALC_CHUNK_SIZE`
- `LOG_LEVEL`

If a command fails because the token is missing, treat that as current behavior rather than the README promise.

## Command Patterns

Use `YYYYMMDD` for `--start-date`, `--end-date`, and `--pick-date` arguments.

Initialize a fresh database:

```bash
cargo run -- init-db
```

Inspect effective config:

```bash
cargo run -- config show
```

Check current coverage and freshness:

```bash
cargo run -- status
```

Run the stock-select contract gate:

```bash
cargo run -- contract-check stock-select \
  --pick-date 20260615 \
  --lookback-trading-days 252 \
  --period-coverage-threshold 0.995
```

Write source caches only:

```bash
cargo run -- write --mode cache --refresh
```

Recompute derived tables from existing cache:

```bash
cargo run -- write \
  --mode calc \
  --start-date 20260601 \
  --end-date 20260626
```

Run a full backfill when both source cache and derived tables need updates:

```bash
cargo run -- write \
  --mode full \
  --start-date 20260601 \
  --end-date 20260626 \
  --max-concurrency 4
```

## Operational Rules

- Use `write --mode cache` for source refreshes and `write --mode calc` for recomputing derived tables from cached source data.
- Use explicit date windows for backfills when possible.
- Keep `--force-recalc` narrow; it can rewrite cumulative period-state rows.
- Use `--refresh` only when the current stock list and Tonghuashun classification need to be refreshed.
- Expect successful `status` and `write` runs to update `runtime/database-state.json`.

## Useful Context

The project maintains these calc outputs:

- `stock_daily_asof_indicators`
- `index_daily_asof_indicators`
- `stock_period_indicator_state`
- `index_period_indicator_state`
- `stock_daily_rolling_factors`
- `stock_daily_left_peak`

For release gates, run `contract-check stock-select` before downstream screening or smoke tests.
