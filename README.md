# stock-cache

`stock-cache` is a PostgreSQL-backed Tushare cache and local factor calculation CLI.

The project has three responsibilities:

- Cache selected Tushare source datasets into PostgreSQL with stable table schemas.
- Compute local stock and index indicators from cached source data.
- Provide precomputed daily, weekly, and monthly MACD state/as-of tables for downstream screening or intraday consumers.

It is intentionally not an analysis or recommendation tool. It does not produce model scores, market opinions, or strategy explanations.

## Layout

```text
deploy/postgres/schema.sql      PostgreSQL schema used by init-db
config/default-indexes.csv      enabled market indexes for index_idx_factor_pro
config/target-cache-tables.txt  enabled target-cache source tables
python/tushare_worker.py        thin Tushare SDK worker process
schemas/tushare-fields.yaml     source of truth for Tushare dataset metadata
src/                            Rust CLI implementation
docs/                           design notes and data usage guides
```

Important docs:

- [Rust CLI design](docs/rust-cli-design.md)
- [Database schema design](docs/database-schema-design.md)
- [Calculation version semantics](docs/calc-version-v1.md)
- [Data usage guide](docs/data-usage-guide.md)
- [Local PostgreSQL setup](docs/local-postgres.md)
- [Generated Tushare field dictionary](docs/tushare-fields.md)

## Requirements

- Rust toolchain
- Docker, for the local PostgreSQL development database
- `uv`, for the Python Tushare worker environment
- A valid `TUSHARE_TOKEN` for cache/full writes

The CLI reads configuration from shell environment variables and optionally from `--env-file`. Shell variables override values in the env file.

Required variables:

```text
POSTGRES_DSN
TUSHARE_TOKEN
```

`TUSHARE_TOKEN` is not required for `status`, `config show`, or `write --mode calc`.

Optional backfill tuning:

```text
CALC_CHUNK_SIZE
CACHE_WARMUP_CONCURRENCY
```

`CALC_CHUNK_SIZE` controls how many pending calc target trading days are computed and written per transaction. If unset, calc keeps windows of 60 or fewer pending trading days together and splits larger backfills into 20-day chunks. The same value can be overridden per run with `write --chunk-size`.

`CACHE_WARMUP_CONCURRENCY` controls how many independent workers fetch calc warmup source data during `write --mode full`. The default is 1 to avoid accidental Tushare rate limiting; use 2 or 4 for an empty-database warmup if your token quota allows it.

## Setup

Start PostgreSQL:

```bash
docker compose -f deploy/docker-compose.yml up -d postgres
```

Create the Python worker environment:

```bash
uv sync --project python
```

Initialize an empty database:

```bash
POSTGRES_DSN='postgres://stock_cache:stock_cache@localhost:5432/stock_cache' \
cargo run -- init-db
```

Show effective config:

```bash
POSTGRES_DSN='postgres://stock_cache:stock_cache@localhost:5432/stock_cache' \
TUSHARE_TOKEN='...' \
cargo run -- config show
```

## Common Commands

Check database coverage:

```bash
POSTGRES_DSN='postgres://stock_cache:stock_cache@localhost:5432/stock_cache' \
cargo run -- status
```

Refresh current stock list and Tonghuashun classification:

```bash
POSTGRES_DSN='postgres://stock_cache:stock_cache@localhost:5432/stock_cache' \
TUSHARE_TOKEN='...' \
cargo run -- write --mode cache --refresh
```

Backfill source data and local calculations for a date range:

```bash
POSTGRES_DSN='postgres://stock_cache:stock_cache@localhost:5432/stock_cache' \
TUSHARE_TOKEN='...' \
cargo run -- write \
  --mode full \
  --start-date 2026-06-01 \
  --end-date 2026-06-26 \
  --max-concurrency 4
```

Recompute local calculation tables from already cached source data:

```bash
POSTGRES_DSN='postgres://stock_cache:stock_cache@localhost:5432/stock_cache' \
cargo run -- write \
  --mode calc \
  --start-date 2026-06-01 \
  --end-date 2026-06-26
```

Every successful `write` or `status` command also saves its summary JSON to:

```text
runtime/database-state.json
```

This file is the latest local database state snapshot from the last successful `write` or `status`, useful for seeing which window and row counts are already cached or calculated. Running `status` overwrites it with a direct read of the current database.

Force recalculation for a small window:

```bash
POSTGRES_DSN='postgres://stock_cache:stock_cache@localhost:5432/stock_cache' \
cargo run -- write \
  --mode calc \
  --start-date 2026-06-26 \
  --end-date 2026-06-26 \
  --force-recalc
```

`--force-recalc` can rewrite a large number of period-state rows because period state is cumulative up to the target date. Prefer narrow windows unless a full rebuild is intentional.

Run the read-only `stock-select` DB-native contract gate before a downstream release:

```bash
POSTGRES_DSN='postgres://stock_cache:stock_cache@localhost:5432/stock_cache' \
cargo run -- contract-check stock-select \
  --pick-date 20260615 \
  --lookback-trading-days 252 \
  --period-coverage-threshold 0.995
```

The check prints JSON and does not print the DSN or token. It verifies source freshness, qfq OHLC coverage, `adj_factor`, KDJ K/D/J semantics, calculation table row alignment, period-state coverage, and THS table availability.

For KDJ, stock-cache keeps Tushare source names: `kdj_qfq` is the J value and must match `3 * kdj_k_qfq - 2 * kdj_d_qfq` within tolerance. Downstream readers that require `kdj_j_qfq` should alias or derive it in their own query.

After this gate passes, run the `stock-select` smoke against the same DB-native contract:

```bash
stock-select screen --method b2 --pick-date 2026-06-15 --recompute --export-factors
stock-select screen --method b2 --intraday --pick-date 2026-06-16 --recompute --export-factors --environment-state neutral --environment-reason smoke
```

The EOD smoke should record prepared metadata `source_table = stock-cache-db-native`.
The intraday artifact should include `intraday_price_live_qfq` and live daily/weekly/monthly MACD fields.

## Backfill Notes

For production-style backfill, prefer `write --mode full` with an explicit date window. `full` caches source tables first, then computes local calculation tables. It also fetches the warmup source range needed by long-period MACD calculations.

Current daily cache scope:

```text
stock_stk_factor_pro
stock_moneyflow_ths
index_idx_factor_pro
index_ths_daily
index_moneyflow_cnt_ths
index_limit_cpt_list
stock_limit_list_ths
```

`index_ths_daily` and `index_moneyflow_cnt_ths` are requested in the same shape as the Python Tushare functions: by `trade_date` only, without enumerating `ts_code`.

## Consuming Calculated Data

Use these tables for downstream readers:

- `stock_daily_asof_indicators`: stock daily/weekly/monthly MACD as-of snapshots.
- `index_daily_asof_indicators`: Tonghuashun sector and configured market index MACD as-of snapshots.
- `stock_period_indicator_state`: completed stock daily/weekly/monthly MACD recursive state.
- `index_period_indicator_state`: completed index daily/weekly/monthly MACD recursive state.
- `stock_daily_rolling_factors`: stock rolling moving average, position, volume, turnover, and range compression factors.
- `stock_daily_left_peak`: stock left-peak structure factors.
- `stock_daily_czsc_structure`: point-in-time CZSC BI/ZS, recent peak, and first-bear snapshots from qfq OHLC.

For EOD screening, prefer the `*_daily_asof_indicators` tables. For intraday simulation, use the latest completed period state plus the live/current qfq price and apply the formula documented in [Data usage guide](docs/data-usage-guide.md).

For weekly/monthly period state, use the latest row with `period_end_date <= pick_date`.
Very short-history stocks may lack weekly or monthly state; downstream consumers should skip those rows when the contract report marks the gap as acceptable.

The temporary `backfill-czsc-structure` binary is intentionally separate from the long-lived CLI. It defaults to the latest 252 market trade dates, replays each stock from its earliest local history, skips dates already complete for `czsc_structure_qfq_min6_r7_v1`, and supports `--dry-run` and explicit `--force` overwrite.

## Development Checks

```bash
cargo fmt --check
cargo test
```

Generated metadata is committed. Update `schemas/tushare-fields.yaml` first, then regenerate derived files with the scripts under `tools/`.
