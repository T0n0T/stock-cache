# Write Date Range Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add CLI support for overriding the default write lookback window with `--lookback-trading-days` or an absolute `--start-date/--end-date` range while keeping the existing default behavior unchanged.

**Architecture:** Keep the CLI thin by parsing and validating write-range options in `src/cli.py`, then pass a small range-spec object into `WriteMarketDataUseCase`. Extend the provider layer with a date-range trade-calendar query so the use case can resolve either a recent lookback or an explicit absolute window without embedding exchange-calendar logic in the use case.

**Tech Stack:** Python 3.13, Typer CLI, Pydantic settings, Tushare provider adapter, pytest

---

### Task 1: Add failing tests for write-range option parsing

**Files:**
- Modify: `tests/test_cli.py`

- [ ] **Step 1: Write the failing tests**

```python
def test_cli_write_accepts_lookback_override() -> None:
    ...

def test_cli_write_accepts_absolute_date_range() -> None:
    ...

def test_cli_write_rejects_mixing_lookback_and_date_range() -> None:
    ...

def test_cli_write_requires_complete_date_range() -> None:
    ...
```

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest tests/test_cli.py -k "lookback_override or absolute_date_range or mixing_lookback_and_date_range or complete_date_range"`
Expected: FAIL because `write` does not yet accept or validate the new options.

- [ ] **Step 3: Write minimal implementation**

```python
def write(..., lookback_trading_days: int | None = ..., start_date: str | None = ..., end_date: str | None = ...) -> None:
    write_range = _validate_write_range(...)
    payload = asyncio.run(_run_write(..., write_range=write_range, ...))
```

- [ ] **Step 4: Run test to verify it passes**

Run: `uv run pytest tests/test_cli.py -k "lookback_override or absolute_date_range or mixing_lookback_and_date_range or complete_date_range"`
Expected: PASS

### Task 2: Add failing tests for use-case trade-date resolution

**Files:**
- Modify: `tests/use_cases/test_write_market_data.py`

- [ ] **Step 1: Write the failing tests**

```python
async def test_write_use_case_uses_cli_lookback_override(...) -> None:
    ...

async def test_write_use_case_uses_absolute_trade_date_range(...) -> None:
    ...
```

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest tests/use_cases/test_write_market_data.py -k "cli_lookback_override or absolute_trade_date_range"`
Expected: FAIL because the use case only supports settings-based lookback.

- [ ] **Step 3: Write minimal implementation**

```python
async def run(self, mode: str, symbols: list[str] | None = None, write_range: WriteDateRange | None = None) -> JobRunSummary:
    trade_dates = self._trade_dates(write_range=write_range) if target_symbols else []
```

- [ ] **Step 4: Run test to verify it passes**

Run: `uv run pytest tests/use_cases/test_write_market_data.py -k "cli_lookback_override or absolute_trade_date_range"`
Expected: PASS

### Task 3: Add provider support for absolute trade-date ranges

**Files:**
- Modify: `src/providers/tushare_adapter.py`
- Modify: `tests/test_cli.py`

- [ ] **Step 1: Write the failing test**

```python
def test_tushare_adapter_fetch_trade_dates_in_range_returns_open_days(monkeypatch) -> None:
    ...
```

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest tests/test_cli.py -k "fetch_trade_dates_in_range_returns_open_days"`
Expected: FAIL because the adapter has no date-range trade-calendar method.

- [ ] **Step 3: Write minimal implementation**

```python
def fetch_trade_dates_in_range(self, start_date: str, end_date: str) -> Sequence[str]:
    frame = self._safe_query(self._pro.trade_cal, exchange="SSE", start_date=start_date, end_date=end_date)
    return [str(row["cal_date"]) for row in frame.to_dict("records") if int(row.get("is_open", 0)) == 1]
```

- [ ] **Step 4: Run test to verify it passes**

Run: `uv run pytest tests/test_cli.py -k "fetch_trade_dates_in_range_returns_open_days"`
Expected: PASS

### Task 4: Update docs and skills, then verify end-to-end

**Files:**
- Modify: `README.md`
- Modify: `skills/stock-cache-write/SKILL.md`

- [ ] **Step 1: Update usage docs**

```markdown
uv run stock-cache write --mode full --lookback-trading-days 30
uv run stock-cache write --mode full --start-date 2026-01-01 --end-date 2026-03-31
```

- [ ] **Step 2: Run focused verification**

Run: `uv run pytest tests/test_cli.py tests/use_cases/test_write_market_data.py`
Expected: PASS

- [ ] **Step 3: Run CLI help verification**

Run: `uv run stock-cache --help`
Expected: PASS with `write` command still listed

Run: `uv run stock-cache write --help`
Expected: PASS with `--lookback-trading-days`, `--start-date`, and `--end-date`
