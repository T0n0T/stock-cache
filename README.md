# stock-cache

`stock-cache` is a Rust-owned Tushare ingestion, DuckDB storage, and local factor calculation tool. The active development target is v3 and it uses a local DuckDB file.

The Rust executable owns HTTP requests, JSON decoding, batch writes, MACD/KDJ/rolling/left-peak/CZSC calculations, and DuckDB transactions.

## Layout

```text
src/main.rs                  CLI root and command dispatch
src/cli/                     command-line argument definitions
src/app/                     write, erase, status, and calculation workflows
src/calc/                    MACD, KDJ, rolling, left-peak, and CZSC algorithms
src/source/                  Rust Tushare HTTP client
src/storage/duckdb/          DuckDB schema, writer, cache, and checkpoint access
src/metadata/                dataset and field metadata
schemas/                     source metadata definition
config/                      default cache table and index lists
docs/program-design.md       runtime and write strategy
docs/schema-design.md        table and performance design
docs/data-usage-guide.md     DuckDB consumer queries
```

`app` coordinates the workflows, `calc` contains reusable calculation logic, and
`storage` owns database operations. The directory names describe these boundaries;
the calculation implementation is not tied to a schema or release version.

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

The public commands are `init` (`init-db` alias), `status`, `write`, and `erase`.
Calculation and HTTP fetching are internal stages of `write`.

## Quick start

```bash
cargo build --bins
cargo run --bin stock-cache -- init --database ./runtime/stock-cache.duckdb
cargo run --bin stock-cache -- status --database ./runtime/stock-cache.duckdb
```

Erase an inclusive date window:

```bash
stock-cache erase --start-date 2026-11-09 --end-date 2026-11-13
```

Erasure removes dated source rows, calculation rows (period tables by period end),
and request coverage in one transaction. It preserves reference snapshots and the
trading calendar. Dates are required; there is no implicit full-database erasure.
Later calculation dates remain stored, matching v2; recalculate through the latest
cached date after correcting history, because rolling results may depend on it.

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

See the design and data-usage documents for calculation semantics, warmup rules, measured ingestion boundaries, and consumer SQL.

The writer benchmark uses a developer example, not the product CLI:

```bash
cargo build --release --example writer-benchmark
python3 tools/benchmark_rust_writer.py
```
