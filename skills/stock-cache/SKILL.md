---
name: stock-cache
description: Operate the Rust v3 Tushare and DuckDB CLI in this repository.
---

# Stock Cache v3

The active path is Rust v3 with a local DuckDB file. Use the binaries directly for maintenance; the Python wrapper only forwards arguments and summarizes JSON output.

## Commands

```bash
cargo run --bin v3-db -- init --database ./runtime/stock-cache-v3.duckdb
cargo run --bin v3-db -- status --database ./runtime/stock-cache-v3.duckdb
cargo run --bin v3-db -- fetch --database ./runtime/stock-cache-v3.duckdb --dataset stock_stk_factor_pro --params '{"trade_date":"20261113"}'
cargo run --bin v3-db -- write --database ./runtime/stock-cache-v3.duckdb --table source_daily_bars --input bars.jsonl --strategy append
cargo run --bin v3-calc --input bars.jsonl --calendar trade_cal.txt --database ./runtime/stock-cache-v3.duckdb --write
```

Set `TUSHARE_TOKEN` for API fetches. `TUSHARE_ENDPOINT`, timeout, retry, and concurrency settings are optional.

## Operational rules

- Use `append` for a new day and `replace-dates` for a backfill or corrected day.
- Use `replace-keys` only when the caller has complete logical keys to remove or replace.
- Include warmup history when recalculating rolling or multi-period indicators.
- Keep `calc_version` explicit when reading or writing calculated tables.
- Keep generated benchmark output under `target/`; do not commit generated reports or local databases.
