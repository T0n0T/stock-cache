# Tushare Bulk Market Sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Switch stock synchronization from per-symbol pulls to trade-date bulk Tushare pulls and persist richer quote and indicator payloads for A-share stocks.

**Architecture:** Keep the existing CLI and repository shape, but change the write path to fetch full-market rows per trade date from Tushare. Merge `daily`, `daily_basic`, `moneyflow`, `adj_factor`, `stk_limit`, `suspend_d`, and `stk_factor_pro`/`stk_factor` into per-symbol rows before upsert, with stable columns mapped explicitly and extra fields stored in JSON columns.

**Tech Stack:** Python 3.13, asyncio, Tushare SDK, asyncpg, pytest, pytest-asyncio

---

### Task 1: Define richer persistence contract

**Files:**
- Modify: `src/domain/models.py`
- Modify: `src/db/schema.sql`
- Test: `tests/repositories/test_market_data_repository.py`

- [ ] Add JSON extra payload support for market and indicator rows, plus fields required for bulk-source provenance.
- [ ] Update SQL schema to persist these columns.
- [ ] Add repository tests that lock the upsert tuple ordering and SQL references.

### Task 2: Add bulk Tushare provider methods

**Files:**
- Modify: `src/providers/tushare_adapter.py`
- Test: `tests/test_cli.py`

- [ ] Add bulk-by-trade-date fetch methods for daily quotes, fundamentals, moneyflow, adjustment factors, limit prices, suspensions, and indicators.
- [ ] Prefer `stk_factor_pro` for indicators and fall back to `stk_factor` on permission-style failures.
- [ ] Add tests that verify real method selection, parameter shape, and record conversion.

### Task 3: Normalize bulk payloads into per-symbol rows

**Files:**
- Modify: `src/services/normalizer.py`
- Test: `tests/services/test_normalizer.py`

- [ ] Replace symbol-local merging with full-market merging keyed by `(ts_code, trade_date)`.
- [ ] Explicitly map core market and indicator columns.
- [ ] Store remaining useful market and indicator fields into JSON payload columns.

### Task 4: Change write orchestration to trade-date bulk sync

**Files:**
- Modify: `src/use_cases/write_market_data.py`
- Test: `tests/use_cases/test_write_market_data.py`

- [ ] Replace the per-symbol loop with a per-trade-date bulk fetch loop.
- [ ] Keep retries at the trade-date batch level.
- [ ] Preserve instrument upsert and status reporting semantics.

### Task 5: Verify targeted behavior

**Files:**
- Test: `tests/test_cli.py`
- Test: `tests/services/test_normalizer.py`
- Test: `tests/repositories/test_market_data_repository.py`
- Test: `tests/use_cases/test_write_market_data.py`

- [ ] Run targeted pytest commands for the touched modules.
- [ ] Fix regressions until the targeted suite is green.
