# Stock Cache Design

Date: 2026-03-30

## 1. Overview

Build a Python CLI tool that fetches A-share market data for all currently listed stocks, including ST and suspended stocks, preferring Tushare and falling back to AKShare when needed. The tool stores recent market data in PostgreSQL, supports structured JSON reads and screening queries, and emits a fixed overwrite status file after each write job. The default historical retention target is the most recent 90 trading days.

This first version is a single-process CLI application with layered modules, not a distributed job system. It must be safe to rerun, support per-symbol retries, and provide a stable boundary for future agent skills.

## 2. Scope

Included in scope:

- CLI-first workflow for write and read operations
- Full A-share symbol universe for currently listed stocks
- Inclusion of ST and suspended stocks
- PostgreSQL persistence
- Tushare as primary source
- AKShare fallback for recoverable source failures
- Recent 90 trading days of history
- Daily market data, daily basic data, money flow, and technical indicators
- Structured JSON read output
- Screening by core market fields and selected indicators
- Fixed overwrite text status file after every write job
- Skill templates for write and read agent invocation
- Extensibility hooks for future custom formula screening

Excluded from scope in v1:

- Real-time streaming quotes
- Distributed workers
- A full custom formula language
- User-facing web UI
- Intraday/minute-level storage
- Broad factor library local recomputation beyond the first required fallback set

## 3. Recommended Architecture

The recommended architecture is a single CLI application with strict internal layering:

- `CLI`
- `Application use cases`
- `Source adapters`
- `Normalizer`
- `Repositories`
- `Infrastructure`

### 3.1 CLI

The CLI parses commands and parameters only. It does not talk directly to external APIs or raw SQL.

Primary commands:

- `write`
- `read raw`
- `read screen`

### 3.2 Application Use Cases

Core use cases:

- `WriteMarketDataUseCase`
- `ReadRawMarketDataUseCase`
- `ReadScreeningResultsUseCase`

The application layer owns orchestration, retries, and domain-level flow. This is the stable boundary that future agent skills should call.

### 3.3 Source Adapters

Adapters normalize upstream provider interaction:

- `TushareAdapter`

They return normalized domain payloads instead of leaking provider-specific field naming into repositories.

### 3.4 Normalizer

The normalizer merges and standardizes provider data from:

- daily market data
- daily basic data
- money flow
- technical factor data

It is responsible for:

- type normalization
- date normalization
- stock code normalization
- field merge by `(ts_code, trade_date)`
- source metadata tagging

### 3.5 Repositories

Repositories isolate persistence concerns:

- `InstrumentRepository`
- `DailyMarketRepository`
- `DailyIndicatorRepository`
- `JobRunRepository`

Repositories implement idempotent upserts and query paths only.

### 3.6 Infrastructure

Infrastructure owns:

- `.env` loading
- async PostgreSQL pool
- retry strategy objects
- logging
- status file writing
- provider client wrappers

## 4. Data Model

The first version uses four main tables.

### 4.1 `instruments`

Purpose: store stock master data and the current symbol universe.

Primary key:

- `ts_code`

Recommended columns:

- `ts_code`
- `symbol`
- `name`
- `area`
- `industry`
- `market`
- `exchange`
- `list_status`
- `list_date`
- `delist_date`
- `is_st`
- `updated_at`

### 4.2 `daily_market`

Purpose: store one merged daily fact row per stock per trading date.

Primary key:

- `(ts_code, trade_date)`

Field groups:

Base market fields:

- `open`
- `high`
- `low`
- `close`
- `pre_close`
- `change`
- `pct_chg`
- `vol`
- `amount`

Daily basic fields:

- `turnover_rate`
- `turnover_rate_f`
- `volume_ratio`
- `pe`
- `pe_ttm`
- `pb`
- `ps`
- `ps_ttm`
- `dv_ratio`
- `dv_ttm`
- `total_share`
- `float_share`
- `free_share`
- `total_mv`
- `circ_mv`

Money flow fields:

- `buy_sm_vol`
- `buy_sm_amount`
- `sell_sm_vol`
- `sell_sm_amount`
- `buy_md_vol`
- `buy_md_amount`
- `sell_md_vol`
- `sell_md_amount`
- `buy_lg_vol`
- `buy_lg_amount`
- `sell_lg_vol`
- `sell_lg_amount`
- `buy_elg_vol`
- `buy_elg_amount`
- `sell_elg_vol`
- `sell_elg_amount`
- `net_mf_vol`
- `net_mf_amount`

Metadata fields:

- `source_provider`
- `source_daily`
- `source_daily_basic`
- `source_moneyflow`
- `ingested_at`
- `updated_at`

### 4.3 `daily_indicators`

Purpose: store selected technical indicators and leave room for future expansion without polluting the core fact table.

Primary key:

- `(ts_code, trade_date)`

First-class explicit columns for high-frequency screening:

- `macd_dif`
- `macd_dea`
- `macd`
- `kdj_k`
- `kdj_d`
- `kdj_j`

Second-class explicit columns for common future extension:

- `rsi_6`
- `rsi_12`
- `rsi_24`
- `boll_upper`
- `boll_mid`
- `boll_lower`
- `cci`

Flexible extension field:

- `extra_factors_jsonb`

Metadata fields:

- `source_provider`
- `source_interface`
- `calc_fallback_used`
- `updated_at`

Indicator sourcing rule:

- Prefer Tushare factor interfaces such as `stk_factor` or `stk_factor_pro`
- If Tushare cannot provide the required indicator data, fall back to local computation for `MACD` and `KDJ`
- The local fallback path must mark `calc_fallback_used = true`

### 4.4 `job_runs`

Purpose: track write-job execution metadata and support auditability and rerun workflows.

Recommended columns:

- `job_id`
- `job_type`
- `started_at`
- `finished_at`
- `status`
- `total_symbols`
- `success_symbols`
- `failed_symbols`
- `status_file_path`
- `error_summary`
- `params_json`

## 5. Indexing Strategy

Required indexes:

`daily_market`

- primary key on `(ts_code, trade_date)`
- index on `(trade_date)`

Initial screening indexes:

- `(trade_date, pct_chg)`
- `(trade_date, turnover_rate)`
- `(trade_date, total_mv)`

`daily_indicators`

- primary key on `(ts_code, trade_date)`

Additional indicator indexes are optional in v1 and should be added only if screening frequency justifies them.

## 6. Write Flow

The write path is a job-oriented workflow built around per-symbol isolation.

### 6.1 Startup Checks

Before any write job starts, the tool must:

- load `.env`
- validate `TUSHARE_TOKEN`
- validate PostgreSQL connection settings
- validate retry settings
- perform a lightweight PostgreSQL connectivity check
- perform a lightweight provider smoke test such as symbol metadata or trade calendar

### 6.2 Symbol Universe Load

The tool loads the current listed A-share universe from Tushare and keeps ST and suspended stocks in scope. It upserts instrument metadata into `instruments` before market-data synchronization begins.

### 6.3 Per-Symbol Fetch Bundle

Each symbol is the smallest retry and success/failure unit.

Each symbol task fetches, for the latest 90 trading days:

- market daily data
- daily basic data
- money flow data
- indicator data

The symbol task then normalizes all datasets into:

- `daily_market` rows
- `daily_indicators` rows

Finally, it performs transactional upserts for that symbol.

This ensures a symbol is either successfully refreshed for the current run or recorded as failed, instead of leaving half-written symbol state.

## 7. Async Concurrency Model

The orchestration model is async at the application level and provider-SDK-compatible at the execution level.

### 7.1 Concurrency Shape

- `asyncio` orchestrates all symbol tasks
- a bounded semaphore enforces `MAX_CONCURRENCY`
- each symbol task may internally fetch its required provider datasets concurrently where safe

### 7.2 Provider Compatibility

If Tushare or AKShare APIs remain synchronous in practice, the application wraps provider calls using `asyncio.to_thread(...)` rather than forcing a custom async HTTP implementation in v1.

This keeps the design practical while preserving async orchestration and bounded concurrency.

## 8. Retry and Fallback Rules

Retry handling must distinguish transient failures from non-retryable failures.

### 8.1 Retryable Failures

Retry only on transient failure classes, such as:

- timeout
- connection reset
- temporary network failures
- 429 or equivalent rate-limit responses
- temporary upstream gateway failures

### 8.2 Non-Retryable Failures

Do not retry:

- parameter validation errors
- permissions or token scope errors
- missing field definitions
- schema mismatch bugs

### 8.3 Retry Policy

The retry policy is configured via `.env` and uses:

- `MAX_RETRIES`
- `RETRY_BASE_DELAY`
- `RETRY_BACKOFF_FACTOR`
- `RETRY_JITTER`

The policy applies at the symbol-task level.

### 8.4 Provider Fallback

The fallback order is:

1. Tushare primary
2. AKShare fallback for recoverable source failures
3. local indicator calculation fallback for required indicators when provider data is unavailable

The fallback path is provider-aware, interface-aware, and should not silently mix incompatible source semantics without metadata.

## 9. Idempotent Persistence

All write operations must be idempotent.

Rules:

- `daily_market` uses upsert on `(ts_code, trade_date)`
- `daily_indicators` uses upsert on `(ts_code, trade_date)`
- rerunning `write --mode full` is safe
- rerunning only failed symbols is safe

The repository layer must not create duplicate rows for repeated jobs.

## 10. Status File Contract

After every write job, the tool overwrites a fixed text file at the configured status path.

Default path:

- `runtime/last-write-status.txt`

The file is text only in v1.

Recommended format:

```text
job_id: 20260330T120000Z
status: partial_success
started_at: 2026-03-30T12:00:00Z
finished_at: 2026-03-30T12:18:42Z
total_symbols: 5400
success_count: 5372
failed_count: 28

successful_symbols:
000001.SZ
000002.SZ
600519.SH

failed_symbols:
300123.SZ | timeout after retries
600456.SH | tushare permission denied
688789.SH | database write timeout
```

This file is the external contract for humans and agents to inspect the last write result and rerun failures when needed.

## 11. CLI Contract

The project uses `uv run` as the default invocation style.

### 11.1 Write Commands

```bash
uv run stock-cache write --mode full
uv run stock-cache write --mode failed-only
uv run stock-cache write --symbols 000001.SZ,600519.SH
```

Optional write parameters:

- `--trade-date`
- `--lookback-days`
- `--dry-run`
- `--status-file`

### 11.2 Read Raw

```bash
uv run stock-cache read raw --ts-code 000001.SZ --start-date 2026-01-01 --end-date 2026-03-30
```

Expected output:

- JSON to stdout

### 11.3 Read Screen

```bash
uv run stock-cache read screen --trade-date 2026-03-30 --pct-chg-gte 5 --turnover-rate-gte 3 --total-mv-lte 30000000000 --macd-gte 0
```

Expected output:

- JSON to stdout

### 11.4 Exit Codes

- `0` full success
- `1` fatal failure
- `2` partial success for write jobs

## 12. Read and Screening Design

The read path supports two use cases.

### 12.1 Raw Read

`read raw` returns structured market and indicator rows by symbol and date range.

Representative JSON structure:

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

### 12.2 Screening Read

`read screen` returns a JSON array of matching symbols for a trade date and a set of supported filters.

Supported filter families in v1:

- price action
- activity and turnover
- valuation and market cap
- money flow
- selected technical indicators

Supported comparison operators:

- `gte`
- `lte`
- `gt`
- `lt`
- `eq`
- `between`

The screening layer uses an allowlist of supported fields and operators rather than arbitrary SQL expression pass-through.

### 12.3 Indicator Read Policy

Read operations use the following policy:

1. prefer indicator values already stored in PostgreSQL
2. if indicator values required by the query are missing and online backfill is enabled, try Tushare factor retrieval
3. if Tushare cannot provide the required values, compute local fallback indicators for `MACD` and `KDJ`

Read operations remain database-first. They do not become on-demand scraping jobs by default.

## 13. Future Formula Extension Boundary

The first version does not implement a custom formula engine, but it reserves a stable extension boundary.

Planned abstractions:

- `IndicatorProvider`
- `ScreenExpressionEngine`

V1 requirement:

- define interfaces and extension points
- do not implement general custom formula parsing or execution yet

This keeps the architecture ready for future formula screening without expanding v1 scope prematurely.

## 14. Environment Variables

Recommended `.env` variables:

Database:

- `POSTGRES_HOST`
- `POSTGRES_PORT`
- `POSTGRES_DB`
- `POSTGRES_USER`
- `POSTGRES_PASSWORD`
- `POSTGRES_DSN`

Providers:

- `TUSHARE_TOKEN`

Concurrency and retry:

- `MAX_CONCURRENCY=20`
- `MAX_RETRIES=3`
- `RETRY_BASE_DELAY=1.0`
- `RETRY_BACKOFF_FACTOR=2.0`
- `RETRY_JITTER=0.2`
- `REQUEST_TIMEOUT_SECONDS=20`

Task behavior:

- `DEFAULT_LOOKBACK_TRADING_DAYS=90`
- `STATUS_FILE_PATH=runtime/last-write-status.txt`
- `ALLOW_INDICATOR_BACKFILL_ON_READ=true`

Runtime behavior:

- `LOG_LEVEL=INFO`
- `WRITE_BATCH_SIZE=500`
- `ENABLE_TUSHARE_INDICATORS=true`
- `ENABLE_LOCAL_INDICATOR_FALLBACK=true`

## 15. Skill Templates

The current phase designs skill templates and invocation rules only. It does not require immediate installation of live skills.

### 15.1 `stock-cache-write`

Intent coverage:

- sync full A-share recent history
- rerun failed symbols
- refresh specific symbols

Invocation style:

```bash
uv run stock-cache write --mode full
```

Skill behavior rules:

- prefer `.env` defaults
- inspect the fixed status file after write completion
- summarize success count, failure count, failure examples, and the status file path
- do not bypass the CLI with direct SQL or provider calls

### 15.2 `stock-cache-read`

Intent coverage:

- read symbol historical data
- screen symbols by supported fields and indicators

Invocation style:

```bash
uv run stock-cache read raw --ts-code 000001.SZ --start-date 2026-01-01 --end-date 2026-03-30
uv run stock-cache read screen --trade-date 2026-03-30 --pct-chg-gte 5 --turnover-rate-gte 3
```

Skill behavior rules:

- consume JSON from stdout
- use only allowlisted CLI filters
- if the user asks for custom formula screening, clearly state that v1 only reserves the extension boundary

## 16. Testing and Verification Targets

The implementation plan must include tests for:

- single-symbol write success
- multi-symbol concurrent write success
- partial failure status-file overwrite behavior
- rerun of failed symbols from the last status file
- stable JSON shape for `read raw`
- supported screening behavior for:
  - `pct_chg`
  - `turnover_rate`
  - `total_mv`
  - `MACD`
  - `KDJ`
- local fallback computation for `MACD` and `KDJ` when provider indicators are unavailable
- provider fallback from Tushare to AKShare under recoverable failure conditions

## 17. Key Design Decisions

- Use a CLI-first single-process architecture in v1
- Keep the smallest retryable unit at the symbol level
- Store raw daily facts and technical indicators in separate tables
- Prefer Tushare for both market data and indicators
- Fall back to AKShare when needed
- Fall back to local indicator calculation for `MACD` and `KDJ` when provider indicator retrieval fails
- Return JSON from all read operations
- Emit a fixed overwrite text status file after each write job
- Reserve, but do not yet implement, a custom formula screening engine
