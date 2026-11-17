# stock-cache

`stock-cache` is a Rust-owned Tushare ingestion, DuckDB storage, and local factor calculation tool. The active development target is v3 and it uses a local DuckDB file.

The Rust binaries own HTTP requests, JSON decoding, batch writes, MACD/KDJ/rolling/left-peak/CZSC calculations, and DuckDB transactions. `python/v3_cli.py` is an optional thin process wrapper and does not open DuckDB.

## Layout

```text
schemas/tushare-fields.yaml  Tushare dataset metadata and DuckDB field types
src/source/tushare_http.rs    Rust reqwest transport
src/v3/duckdb.rs              DuckDB schema, staging writer, and checkpoint I/O
src/v3/{macd,kdj,rolling,left_peak,czsc}.rs  calculation kernels
src/bin/v3-db.rs              init/status/fetch/write CLI
src/bin/v3-calc.rs            calculation CLI with direct DuckDB persistence
docs/v3-program-design.md     runtime and write strategy
docs/v3-schema-design.md     table and performance design
docs/data-usage-guide.md      DuckDB consumer queries
```

## Quick start

```bash
cargo build --bins
cargo run --bin v3-db -- init --database ./runtime/stock-cache-v3.duckdb
cargo run --bin v3-db -- status --database ./runtime/stock-cache-v3.duckdb
```

Fetch a generated Tushare dataset and write it as one transaction:

```bash
TUSHARE_TOKEN='...' cargo run --bin v3-db -- fetch \
  --database ./runtime/stock-cache-v3.duckdb \
  --dataset stock_stk_factor_pro \
  --params '{"trade_date":"20261113"}'
```

Write an existing JSONL batch with an explicit strategy:

```bash
cargo run --bin v3-db -- write --database ./runtime/stock-cache-v3.duckdb \
  --table source_daily_bars --input bars.jsonl --strategy append
```

Compute and persist v3 tables:

```bash
cargo run --bin v3-calc --input bars-with-warmup.jsonl \
  --calendar trade_cal.txt --database ./runtime/stock-cache-v3.duckdb --write
```

Use `uv run --project python python python/v3_cli.py ...` only when a Python process wrapper is useful.

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
uv run --project python --group test python/test_v3_cli.py
git diff --check
```

See the v3 documents for calculation semantics, warmup rules, measured ingestion boundaries, and consumer SQL.
