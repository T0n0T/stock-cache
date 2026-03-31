# Write Memory Optimization Design

Date: 2026-03-31

## Overview

The current `write --mode full` flow accumulates all trade-date payloads in memory before normalization and persistence. This creates a memory peak proportional to the full write window times the A-share universe times multiple provider payloads.

This change restructures the write path to stream one trade date at a time and persist normalized rows immediately. It also makes repository writes chunked so each `executemany` call only materializes a bounded number of SQL value tuples.

The goal is to reduce peak memory usage without changing the CLI contract, database schema, or job summary output.

## Current Root Cause

In `src/use_cases/write_market_data.py`, the `full` mode currently:

1. Resolves the target symbol universe and trade dates.
2. Fetches seven provider payload groups per trade date.
3. Extends seven process-wide Python lists across the whole date window.
4. Normalizes only after all trade dates are fetched.
5. Persists only after all normalized rows are built.

This keeps several large object graphs live at once:

- raw provider dictionaries for all trade dates
- merged normalization dictionaries
- normalized dataclass rows
- SQL parameter tuples created for repository upserts

The current repository path in `src/repositories/market_data.py` also builds all SQL values for the input list at once, which creates a secondary memory spike during persistence.

## Design Goals

- Reduce peak memory from full-window accumulation to per-trade-date processing.
- Bound repository write memory with configurable chunking.
- Preserve current CLI options and JSON output contracts.
- Preserve current job summary semantics, including trade-date-scoped failures.
- Keep the CLI thin and keep orchestration and persistence concerns in their current layers.

## Non-Goals

- No new CLI commands or top-level entrypoints.
- No schema changes.
- No change to the final `JobRunSummary` fields.
- No PostgreSQL `COPY` or staging-table refactor in this change.
- No rewrite of the single-symbol range-fetch strategy beyond benefiting from chunked repository writes.

## Recommended Architecture

### 1. Stream `full` mode by trade date

`WriteMarketDataUseCase.run(..., mode="full")` should process each trade date independently:

1. Fetch that trade date's seven provider payload groups.
2. Normalize only that trade date's payloads.
3. Persist market rows immediately.
4. Persist indicator rows immediately.
5. Move to the next trade date.

The use case should no longer keep cross-date accumulator lists like `daily_rows`, `daily_basic_rows`, `moneyflow_rows`, `adj_factor_rows`, `limit_rows`, `suspend_rows`, and `indicator_rows`.

This changes memory behavior from full-window retention to bounded per-trade-date retention.

### 2. Keep failure semantics unchanged

If a trade date fails during fetch, normalize, or persist:

- record the failure under `__trade_date__:{trade_date}`
- emit progress for that failed trade date
- continue processing later trade dates

At the end of `full` mode:

- `success_symbols` remains `list(target_symbols)` only when there are no failures
- `status` remains `success` when there are no failures, otherwise `partial_success`

This keeps the existing summary contract unchanged even though persistence becomes incremental.

### 3. Chunk repository upserts

`MarketDataRepository` should perform chunked upserts for both market rows and indicator rows.

The repository should:

- accept the full list passed from the use case
- slice it into chunks of `Settings.write_batch_size`
- build SQL values for one chunk at a time
- call `executemany` once per chunk

This prevents a large single-day input from generating one very large `values` list in memory.

## Detailed Component Changes

### `src/use_cases/write_market_data.py`

Change only the `full` mode orchestration.

Current behavior:

- fetch all dates
- append all raw rows into seven large lists
- normalize once
- persist once

New behavior:

- for each `trade_date`
- fetch payload tuple with retries
- call `normalize_market_batches(...)` for only that trade date's payloads
- persist that bundle immediately through the repository

Helpful extraction is allowed if it keeps the file readable, for example a private helper like `_persist_full_trade_date(...)`, but the logic should remain in this use-case module.

`single` mode should keep its current range-based provider calls. It will automatically benefit from repository chunking because it still persists through the same repository methods.

### `src/repositories/market_data.py`

Add bounded write batching inside:

- `upsert_daily_market`
- `upsert_daily_indicators`

Implementation rules:

- use a private chunk iterator/helper in this module only
- validate or defensively handle non-positive batch sizes so repository behavior remains safe
- keep SQL text and conflict behavior unchanged
- keep JSON serialization behavior unchanged

The repository remains the correct layer for this because write batching is a persistence concern, not a use-case concern.

### `src/config.py`

No schema or contract change is needed.

`write_batch_size` already exists and should become the effective repository write chunk size. No new setting is required.

### `README.md`

Update documentation so `WRITE_BATCH_SIZE` is described as an active write chunking control rather than a reserved field. Also document that `full` writes are processed trade-date by trade-date with chunked upserts to reduce memory pressure.

## Why Not Use PostgreSQL `COPY` Now

`COPY` is not the right first move for this optimization because the current repository contract is upsert-based:

- current writes rely on `INSERT ... ON CONFLICT DO UPDATE`
- direct replacement with `COPY` would require a staging-table or temp-table merge design
- that would expand the scope from memory optimization into a persistence strategy rewrite

Streaming plus chunked `executemany` solves the immediate memory problem while keeping behavior stable and risk contained.

## Testing Strategy

### Use-case tests

Extend `tests/use_cases/test_write_market_data.py` to verify:

- `full` mode persists per trade date instead of once after all dates complete
- later trade dates still process after an earlier trade date fails
- summary fields remain unchanged from the current contract

The test doubles should record how many times market and indicator upserts are called and what rows are passed each time.

### Repository tests

Extend `tests/repositories/test_market_data_repository.py` to verify:

- large input is split into multiple `executemany` calls
- each call receives no more than `WRITE_BATCH_SIZE` rows worth of values
- empty input still returns without database calls

The repository test should use a fake connection that records `executemany` calls.

### Verification commands

Minimum verification for this change:

- `uv run pytest tests/use_cases/test_write_market_data.py tests/repositories/test_market_data_repository.py`
- `uv run stock-cache --help`

If the local PostgreSQL instance is available, an additional targeted live verification is useful but not required for the initial code change because the key behavior change is in orchestration and batching, both of which are covered by unit tests.

## Risks And Mitigations

### Risk: partial persistence changes retry expectations

Because writes happen incrementally, a failed later trade date may leave earlier dates already persisted.

Mitigation:

- this is already consistent with the existing idempotent upsert model
- reruns remain safe because rows are keyed by `(ts_code, trade_date)` and upserted

### Risk: changed progress timing

Progress output will now interleave fetch and persist work per trade date rather than one final persist step.

Mitigation:

- no machine-readable output format changes
- progress is informational stderr output only

### Risk: too-small or invalid batch size

Mitigation:

- repository should defensively clamp or reject invalid sizes before chunking
- default remains the existing `500`

## File Boundaries

Modify:

- `src/use_cases/write_market_data.py`
- `src/repositories/market_data.py`
- `tests/use_cases/test_write_market_data.py`
- `tests/repositories/test_market_data_repository.py`
- `README.md`

Do not modify:

- CLI command signatures
- database schema files
- read-path JSON contracts

## Expected Outcome

After this change, peak memory in `write --mode full` should be bounded primarily by:

- one trade date's fetched provider payloads
- one trade date's normalized rows
- one repository write chunk's SQL values

instead of the full write window's accumulated payloads and values.
