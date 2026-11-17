---
name: stock-cache
description: Operate the Rust Tushare and DuckDB CLI in this repository.
---

# Stock Cache

The production path is Rust with a local DuckDB file. Use the binaries directly for maintenance.

## Commands

```bash
cargo run --bin stock-cache -- init --database ./runtime/stock-cache.duckdb
cargo run --bin stock-cache -- status --database ./runtime/stock-cache.duckdb
cargo run --bin stock-cache -- fetch --database ./runtime/stock-cache.duckdb --dataset stock_stk_factor_pro --params '{"trade_date":"20261113"}'
cargo run --bin stock-cache -- import --database ./runtime/stock-cache.duckdb --table source_daily_bars --input bars.jsonl --strategy append
cargo run --bin stock-cache -- calc --input bars.jsonl --calendar trade_cal.txt --database ./runtime/stock-cache.duckdb --write
```

Set `TUSHARE_TOKEN` for API fetches. `TUSHARE_ENDPOINT`, timeout, retry, and concurrency settings are optional.

## Operational rules

- Use `append` for a new day and `replace-dates` for a backfill or corrected day.
- Use `replace-keys` only when the caller has complete logical keys to remove or replace.
- Include warmup history when recalculating rolling or multi-period indicators.
- Keep `calc_version` explicit when reading or writing calculated tables.
- Keep generated benchmark output under `target/`; do not commit generated reports or local databases.

## Normal market update

Use `stock-cache write --mode full --lookback-trading-days 1` for the normal daily
cache-and-calculate workflow. It fills missing 36-month warmup source history and
persists target results in `runtime/stock-cache.duckdb`. `.env` is loaded by default.
Use `--mode cache` for fetching only, `--mode calc` for calculation from cached data,
and paired `--start-date/--end-date` for a historical window. `import` writes JSONL.
