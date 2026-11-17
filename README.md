# stock-cache

`stock-cache` is a Rust-owned Tushare ingestion, DuckDB storage, and local factor calculation tool. The active development target is v3 and it uses a local DuckDB file.

The Rust executable owns HTTP requests, JSON decoding, batch writes, MACD/KDJ/rolling/left-peak/CZSC calculations, and DuckDB transactions.

## Layout

```text
schemas/tushare-fields.yaml  Tushare dataset metadata and DuckDB field types
src/source/tushare_http.rs    Rust reqwest transport
src/v3/duckdb.rs              DuckDB schema, staging writer, and checkpoint I/O
src/v3/{macd,kdj,rolling,left_peak,czsc}.rs  calculation kernels
src/main.rs                  unified CLI
src/cli/write.rs             market-cache workflow
src/cli/calc.rs              calculation subcommand with DuckDB persistence
docs/program-design.md        runtime and write strategy
docs/schema-design.md         table and performance design
docs/data-usage-guide.md      DuckDB consumer queries
```

## Daily market-cache workflow

```bash
target/release/stock-cache write --mode full --lookback-trading-days 1
# Historical target window:
target/release/stock-cache write --mode full --start-date 2026-11-09 --end-date 2026-11-13
```

`write` is the normal market-cache entry point. `full` fetches target-day market
sources, fills missing calculation-source history for the preceding 36 months,
then calculates and writes the target results. Trading dates come from Tushare's
SSE calendar; a weekend selects the latest open session. If the target day's stock
data has not been published, the command fails instead of silently using yesterday.

The default database is `runtime/stock-cache.duckdb` (`DUCKDB_PATH` or `--database`
overrides it). Shell variables take priority over `.env`; `--env-file` selects a
specific file. `cache` fetches only target sources; `calc` calculates from existing
cached sources and calendar without HTTP access. `--refresh` updates reference
snapshots. `--max-concurrency` bounds HTTP requests; `--chunk-size` bounds the number
of instruments per calculation batch. `--force-recalc` also refetches history in
`full` mode. Targets are refreshed on every run; successful warmup requests are
reused, including empty responses. Newly added indexes receive their own backfill.

Stocks use `*_qfq` OHLC; indexes use their raw OHLC. Warmup rows remain in source
tables; only target-date calculation rows and completed-period rows ending in the
target window are persisted. Existing earlier cache history remains part of the
calculation input so EMA seeds do not shift forward each day. The initial 36-month
warmup follows v2's finite-history convention, not an inception-history baseline.
Writes use Rust batch staging and transactions; failure leaves completed source
batches reusable for a retry. A workflow is not one transaction across all HTTP
requests and calculation batches.

`import` is the low-level JSONL writer; `fetch` and `calc` remain diagnostic tools.

## Quick start

```bash
cargo build --bins
cargo run --bin stock-cache -- init --database ./runtime/stock-cache.duckdb
cargo run --bin stock-cache -- status --database ./runtime/stock-cache.duckdb
```

Fetch a generated Tushare dataset and write it as one transaction:

```bash
TUSHARE_TOKEN='...' cargo run --bin stock-cache -- fetch \
  --database ./runtime/stock-cache.duckdb \
  --dataset stock_stk_factor_pro \
  --params '{"trade_date":"20261113"}'
```

Write an existing JSONL batch with an explicit strategy:

```bash
cargo run --bin stock-cache -- import --database ./runtime/stock-cache.duckdb \
  --table source_daily_bars --input bars.jsonl --strategy append
```

Compute and persist v3 tables:

```bash
cargo run --bin stock-cache -- calc --input bars-with-warmup.jsonl \
  --calendar trade_cal.txt --database ./runtime/stock-cache.duckdb --write
```

## Write strategies

- Daily append: `append` for a new logical-key range.
- Historical completion or a corrected date: `replace-dates` with an explicit date set.
- Explicit row deletion or replacement: `replace-keys` with complete logical keys.

The writer loads a JSONL batch into DuckDB staging, deduplicates by logical key, applies the date/key replacement, commits once, and checkpoints. It does not perform production row-by-row upserts.

## Verification

```bash
cargo fmt --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
git diff --check
```

See the v3 documents for calculation semantics, warmup rules, measured ingestion boundaries, and consumer SQL.
