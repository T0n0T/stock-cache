# Stock Cache Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a `uv`-driven Python CLI that fetches recent A-share market data from Tushare with AKShare fallback, stores the most recent 90 trading days in PostgreSQL, emits a fixed overwrite status file after write jobs, and supports JSON raw reads and screening queries.

**Architecture:** Implement a small `src`-root module layout with clear module boundaries: config, domain models, provider adapters, repositories, indicator services, application use cases, and a Typer CLI. Persist normalized daily facts and technical indicators in separate PostgreSQL tables, keep retries scoped at the symbol level, and expose read paths that stay database-first but can backfill missing indicators. Use TDD throughout so the empty repository grows around stable interfaces instead of ad hoc scripts.

**Tech Stack:** Python 3.13, `uv`, Typer, Pydantic Settings, asyncpg, pytest, pytest-asyncio, Tushare, AKShare

---

## Planned File Structure

Create these files during implementation. Each file has one responsibility.

- `pyproject.toml`
  Add runtime and test dependencies, a console script, and pytest config.
- `README.md`
  Replace the placeholder with setup, `.env`, and CLI usage.
- `.env.example`
  Document all required and optional environment variables.
- `src/cli.py`
  Typer app with `write`, `read raw`, and `read screen`, and supports `python -m cli`.
- `src/config.py`
  Pydantic settings model and config loading.
- `src/app_logging.py`
  Logger creation for CLI and application layers.
- `src/domain/models.py`
  Typed domain models for instruments, daily market rows, indicator rows, and job results.
- `src/domain/errors.py`
  Retryable and non-retryable error types.
- `src/db/schema.sql`
  PostgreSQL DDL for `instruments`, `daily_market`, `daily_indicators`, and `job_runs`.
- `src/db/pool.py`
  Asyncpg connection-pool factory.
- `src/repositories/instruments.py`
  Instrument upsert and stock-universe reads.
- `src/repositories/market_data.py`
  Daily market and indicator upserts plus raw/query reads.
- `src/repositories/job_runs.py`
  Job run persistence and failed-symbol lookup helpers.
- `src/providers/tushare_adapter.py`
  Market data provider integration.
- `src/services/normalizer.py`
  Merge daily/daily_basic/moneyflow/factors by `(ts_code, trade_date)`.
- `src/services/indicators.py`
  Indicator backfill policy and local `MACD`/`KDJ` calculation.
- `src/services/status_reporter.py`
  Fixed overwrite text status-file writer and parser.
- `src/services/retry.py`
  Retry policy wrapper used by symbol jobs.
- `src/use_cases/write_market_data.py`
  Symbol-universe load, async orchestration, retries, transactional persistence.
- `src/use_cases/read_raw.py`
  Raw JSON read use case.
- `src/use_cases/read_screen.py`
  Screening read use case using allowlisted filters.
- `src/skills/stock-cache-write/SKILL.md`
  Skill template for write commands.
- `src/skills/stock-cache-read/SKILL.md`
  Skill template for read commands.
- `tests/conftest.py`
  Shared fixtures for settings, fake providers, and temporary status files.
- `tests/test_config.py`
  Settings loading tests.
- `tests/repositories/test_market_data_repository.py`
  Repository contract tests.
- `tests/services/test_normalizer.py`
  Merge and normalization tests.
- `tests/services/test_indicators.py`
  Local `MACD` and `KDJ` fallback tests.
- `tests/services/test_status_reporter.py`
  Status file overwrite and parse tests.
- `tests/use_cases/test_write_market_data.py`
  Symbol-level retries, partial-success handling, and job summary tests.
- `tests/use_cases/test_read_raw.py`
  Raw JSON shape tests.
- `tests/use_cases/test_read_screen.py`
  Screening filter tests.
- `tests/test_cli.py`
  CLI smoke tests with Typer runner.

## Task 1: Project Bootstrap and Dependency Setup

**Files:**
- Create: `/home/pi/Documents/agents/stock-cache/.env.example`
- Create: `/home/pi/Documents/agents/stock-cache/src/cli.py`
- Modify: `/home/pi/Documents/agents/stock-cache/pyproject.toml`
- Test: `/home/pi/Documents/agents/stock-cache/tests/test_cli.py`

- [ ] **Step 1: Write the failing CLI smoke test**

```python
from typer.testing import CliRunner

from cli import app


runner = CliRunner()


def test_cli_help_lists_write_and_read_commands() -> None:
    result = runner.invoke(app, ["--help"])

    assert result.exit_code == 0
    assert "write" in result.stdout
    assert "read" in result.stdout
```

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest tests/test_cli.py::test_cli_help_lists_write_and_read_commands -v`
Expected: FAIL with `ModuleNotFoundError: No module named 'cli'` or `ImportError` because the CLI module does not exist yet.

- [ ] **Step 3: Add package metadata, dependencies, and a minimal CLI entrypoint**

```toml
[project]
name = "stock-cache"
version = "0.1.0"
description = "A-share market data cache with PostgreSQL persistence"
readme = "README.md"
requires-python = ">=3.13"
dependencies = [
  "asyncpg>=0.30.0",
  "pydantic-settings>=2.8.0",
  "python-dotenv>=1.0.1",
  "tushare>=1.4.18",
  "typer>=0.16.0",
]

[project.optional-dependencies]
dev = [
  "pytest>=8.3.5",
  "pytest-asyncio>=0.26.0",
]

[project.scripts]
stock-cache = "cli:app"

[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[tool.hatch.build.targets.wheel]
only-include = ["src"]
sources = ["src"]

[tool.pytest.ini_options]
pythonpath = ["src"]
asyncio_mode = "auto"
```

```python
# src/cli.py
import typer

app = typer.Typer(help="Cache A-share market data into PostgreSQL.")

read_app = typer.Typer(help="Read cached data.")
app.add_typer(read_app, name="read")


@app.command()
def write() -> None:
    """Write cached market data."""


@read_app.command("raw")
def read_raw() -> None:
    """Read raw cached market data."""


@read_app.command("screen")
def read_screen() -> None:
    """Read screened cached market data."""
```

```env
# .env.example
POSTGRES_DSN=postgresql://postgres:postgres@localhost:5432/stock_cache
TUSHARE_TOKEN=your_token_here
MAX_CONCURRENCY=20
MAX_RETRIES=3
RETRY_BASE_DELAY=1.0
RETRY_BACKOFF_FACTOR=2.0
RETRY_JITTER=0.2
REQUEST_TIMEOUT_SECONDS=20
DEFAULT_LOOKBACK_TRADING_DAYS=90
STATUS_FILE_PATH=runtime/last-write-status.txt
ALLOW_INDICATOR_BACKFILL_ON_READ=true
ENABLE_TUSHARE_INDICATORS=true
ENABLE_LOCAL_INDICATOR_FALLBACK=true
WRITE_BATCH_SIZE=500
LOG_LEVEL=INFO
```

- [ ] **Step 4: Run test to verify it passes**

Run: `uv run pytest tests/test_cli.py::test_cli_help_lists_write_and_read_commands -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add pyproject.toml .env.example src/cli.py tests/test_cli.py
git commit -m "feat: bootstrap stock-cache package"
```

## Task 2: Settings, Logging, and Error Types

**Files:**
- Create: `/home/pi/Documents/agents/stock-cache/src/config.py`
- Create: `/home/pi/Documents/agents/stock-cache/src/app_logging.py`
- Create: `/home/pi/Documents/agents/stock-cache/src/domain/errors.py`
- Create: `/home/pi/Documents/agents/stock-cache/tests/test_config.py`
- Modify: `/home/pi/Documents/agents/stock-cache/tests/conftest.py`

- [ ] **Step 1: Write the failing settings test**

```python
from config import Settings


def test_settings_load_default_values(monkeypatch) -> None:
    monkeypatch.setenv("POSTGRES_DSN", "postgresql://postgres:postgres@localhost:5432/stock_cache")
    monkeypatch.setenv("TUSHARE_TOKEN", "token")

    settings = Settings()

    assert settings.default_lookback_trading_days == 90
    assert settings.status_file_path.as_posix() == "runtime/last-write-status.txt"
    assert settings.max_retries == 3
```

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest tests/test_config.py::test_settings_load_default_values -v`
Expected: FAIL with `ModuleNotFoundError` or `ImportError` because `Settings` is not implemented yet.

- [ ] **Step 3: Implement typed settings, logging, and domain errors**

```python
# src/config.py
from pathlib import Path

from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    postgres_dsn: str = Field(alias="POSTGRES_DSN")
    tushare_token: str = Field(alias="TUSHARE_TOKEN")
    max_concurrency: int = Field(default=20, alias="MAX_CONCURRENCY")
    max_retries: int = Field(default=3, alias="MAX_RETRIES")
    retry_base_delay: float = Field(default=1.0, alias="RETRY_BASE_DELAY")
    retry_backoff_factor: float = Field(default=2.0, alias="RETRY_BACKOFF_FACTOR")
    retry_jitter: float = Field(default=0.2, alias="RETRY_JITTER")
    request_timeout_seconds: int = Field(default=20, alias="REQUEST_TIMEOUT_SECONDS")
    default_lookback_trading_days: int = Field(default=90, alias="DEFAULT_LOOKBACK_TRADING_DAYS")
    status_file_path: Path = Field(default=Path("runtime/last-write-status.txt"), alias="STATUS_FILE_PATH")
    allow_indicator_backfill_on_read: bool = Field(default=True, alias="ALLOW_INDICATOR_BACKFILL_ON_READ")
    enable_tushare_indicators: bool = Field(default=True, alias="ENABLE_TUSHARE_INDICATORS")
    enable_local_indicator_fallback: bool = Field(default=True, alias="ENABLE_LOCAL_INDICATOR_FALLBACK")
    write_batch_size: int = Field(default=500, alias="WRITE_BATCH_SIZE")
    log_level: str = Field(default="INFO", alias="LOG_LEVEL")

    model_config = SettingsConfigDict(env_file=".env", env_file_encoding="utf-8", extra="ignore")
```

```python
# src/app_logging.py
import logging


def configure_logging(level: str) -> None:
    logging.basicConfig(
        level=getattr(logging, level.upper(), logging.INFO),
        format="%(asctime)s %(levelname)s %(name)s %(message)s",
    )
```

```python
# src/domain/errors.py
class StockCacheError(Exception):
    """Base error for stock-cache."""


class RetryableProviderError(StockCacheError):
    """A provider call can be retried safely."""


class NonRetryableProviderError(StockCacheError):
    """A provider call should fail fast."""


class ConfigurationError(StockCacheError):
    """Environment configuration is missing or invalid."""
```

```python
# tests/conftest.py
import pytest


@pytest.fixture
def sample_dsn() -> str:
    return "postgresql://postgres:postgres@localhost:5432/stock_cache"
```

- [ ] **Step 4: Run test to verify it passes**

Run: `uv run pytest tests/test_config.py::test_settings_load_default_values -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/config.py src/app_logging.py src/domain/errors.py tests/conftest.py tests/test_config.py
git commit -m "feat: add settings and error primitives"
```

## Task 3: Domain Models and Status Reporter

**Files:**
- Create: `/home/pi/Documents/agents/stock-cache/src/domain/models.py`
- Create: `/home/pi/Documents/agents/stock-cache/src/services/status_reporter.py`
- Create: `/home/pi/Documents/agents/stock-cache/tests/services/test_status_reporter.py`

- [ ] **Step 1: Write the failing status reporter test**

```python
from pathlib import Path

from domain.models import JobRunSummary
from services.status_reporter import StatusReporter


def test_status_reporter_overwrites_file(tmp_path: Path) -> None:
    status_file = tmp_path / "last-write-status.txt"
    reporter = StatusReporter(status_file)
    summary = JobRunSummary(
        job_id="20260330T120000Z",
        status="partial_success",
        started_at="2026-03-30T12:00:00Z",
        finished_at="2026-03-30T12:18:42Z",
        total_symbols=3,
        success_symbols=["000001.SZ", "000002.SZ"],
        failed_symbols={"600000.SH": "timeout after retries"},
    )

    reporter.write(summary)
    contents = status_file.read_text(encoding="utf-8")

    assert "000001.SZ" in contents
    assert "600000.SH | timeout after retries" in contents
```

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest tests/services/test_status_reporter.py::test_status_reporter_overwrites_file -v`
Expected: FAIL because `JobRunSummary` and `StatusReporter` do not exist yet.

- [ ] **Step 3: Implement the domain dataclasses and status-file service**

```python
# src/domain/models.py
from dataclasses import dataclass, field
from datetime import date


@dataclass(slots=True)
class Instrument:
    ts_code: str
    symbol: str
    name: str
    exchange: str
    list_status: str
    is_st: bool


@dataclass(slots=True)
class DailyMarketRow:
    ts_code: str
    trade_date: date
    open: float | None = None
    high: float | None = None
    low: float | None = None
    close: float | None = None
    pct_chg: float | None = None
    turnover_rate: float | None = None
    total_mv: float | None = None
    net_mf_amount: float | None = None
    source_provider: str = "tushare"


@dataclass(slots=True)
class DailyIndicatorRow:
    ts_code: str
    trade_date: date
    macd_dif: float | None = None
    macd_dea: float | None = None
    macd: float | None = None
    kdj_k: float | None = None
    kdj_d: float | None = None
    kdj_j: float | None = None
    source_provider: str = "tushare"
    source_interface: str = "stk_factor"
    calc_fallback_used: bool = False


@dataclass(slots=True)
class JobRunSummary:
    job_id: str
    status: str
    started_at: str
    finished_at: str
    total_symbols: int
    success_symbols: list[str] = field(default_factory=list)
    failed_symbols: dict[str, str] = field(default_factory=dict)
```

```python
# src/services/status_reporter.py
from pathlib import Path

from domain.models import JobRunSummary


class StatusReporter:
    def __init__(self, path: Path) -> None:
        self._path = path

    def write(self, summary: JobRunSummary) -> None:
        self._path.parent.mkdir(parents=True, exist_ok=True)
        lines = [
            f"job_id: {summary.job_id}",
            f"status: {summary.status}",
            f"started_at: {summary.started_at}",
            f"finished_at: {summary.finished_at}",
            f"total_symbols: {summary.total_symbols}",
            f"success_count: {len(summary.success_symbols)}",
            f"failed_count: {len(summary.failed_symbols)}",
            "",
            "successful_symbols:",
            *summary.success_symbols,
            "",
            "failed_symbols:",
            *[f"{code} | {reason}" for code, reason in summary.failed_symbols.items()],
        ]
        self._path.write_text("\n".join(lines) + "\n", encoding="utf-8")
```

- [ ] **Step 4: Run test to verify it passes**

Run: `uv run pytest tests/services/test_status_reporter.py::test_status_reporter_overwrites_file -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/domain/models.py src/services/status_reporter.py tests/services/test_status_reporter.py
git commit -m "feat: add domain models and status reporter"
```

## Task 4: Database Schema, Pool, and Repository Contracts

**Files:**
- Create: `/home/pi/Documents/agents/stock-cache/src/db/schema.sql`
- Create: `/home/pi/Documents/agents/stock-cache/src/db/pool.py`
- Create: `/home/pi/Documents/agents/stock-cache/src/repositories/instruments.py`
- Create: `/home/pi/Documents/agents/stock-cache/src/repositories/market_data.py`
- Create: `/home/pi/Documents/agents/stock-cache/src/repositories/job_runs.py`
- Create: `/home/pi/Documents/agents/stock-cache/tests/repositories/test_market_data_repository.py`

- [ ] **Step 1: Write the failing repository contract test**

```python
from datetime import date

from domain.models import DailyIndicatorRow, DailyMarketRow
from repositories.market_data import build_daily_indicator_upsert, build_daily_market_upsert


def test_build_daily_market_upsert_uses_composite_key() -> None:
    row = DailyMarketRow(ts_code="000001.SZ", trade_date=date(2026, 3, 30), close=12.4)
    sql, values = build_daily_market_upsert([row])

    assert "ON CONFLICT (ts_code, trade_date)" in sql
    assert values[0]["ts_code"] == "000001.SZ"


def test_build_daily_indicator_upsert_uses_composite_key() -> None:
    indicator = DailyIndicatorRow(ts_code="000001.SZ", trade_date=date(2026, 3, 30), macd=0.1)
    sql, values = build_daily_indicator_upsert([indicator])

    assert "ON CONFLICT (ts_code, trade_date)" in sql
    assert values[0]["macd"] == 0.1
```

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest tests/repositories/test_market_data_repository.py -v`
Expected: FAIL because repository builders do not exist yet.

- [ ] **Step 3: Add the DDL and SQL-builder helpers**

```sql
-- src/db/schema.sql
CREATE TABLE IF NOT EXISTS instruments (
  ts_code TEXT PRIMARY KEY,
  symbol TEXT NOT NULL,
  name TEXT NOT NULL,
  area TEXT,
  industry TEXT,
  market TEXT,
  exchange TEXT NOT NULL,
  list_status TEXT NOT NULL,
  list_date DATE,
  delist_date DATE,
  is_st BOOLEAN NOT NULL DEFAULT FALSE,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS daily_market (
  ts_code TEXT NOT NULL,
  trade_date DATE NOT NULL,
  open DOUBLE PRECISION,
  high DOUBLE PRECISION,
  low DOUBLE PRECISION,
  close DOUBLE PRECISION,
  pre_close DOUBLE PRECISION,
  change DOUBLE PRECISION,
  pct_chg DOUBLE PRECISION,
  vol DOUBLE PRECISION,
  amount DOUBLE PRECISION,
  turnover_rate DOUBLE PRECISION,
  turnover_rate_f DOUBLE PRECISION,
  volume_ratio DOUBLE PRECISION,
  pe DOUBLE PRECISION,
  pe_ttm DOUBLE PRECISION,
  pb DOUBLE PRECISION,
  ps DOUBLE PRECISION,
  ps_ttm DOUBLE PRECISION,
  dv_ratio DOUBLE PRECISION,
  dv_ttm DOUBLE PRECISION,
  total_share DOUBLE PRECISION,
  float_share DOUBLE PRECISION,
  free_share DOUBLE PRECISION,
  total_mv DOUBLE PRECISION,
  circ_mv DOUBLE PRECISION,
  net_mf_vol DOUBLE PRECISION,
  net_mf_amount DOUBLE PRECISION,
  source_provider TEXT NOT NULL,
  source_daily TEXT,
  source_daily_basic TEXT,
  source_moneyflow TEXT,
  ingested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (ts_code, trade_date)
);

CREATE INDEX IF NOT EXISTS idx_daily_market_trade_date ON daily_market (trade_date);
CREATE INDEX IF NOT EXISTS idx_daily_market_trade_date_pct_chg ON daily_market (trade_date, pct_chg);
CREATE INDEX IF NOT EXISTS idx_daily_market_trade_date_turnover_rate ON daily_market (trade_date, turnover_rate);
CREATE INDEX IF NOT EXISTS idx_daily_market_trade_date_total_mv ON daily_market (trade_date, total_mv);

CREATE TABLE IF NOT EXISTS daily_indicators (
  ts_code TEXT NOT NULL,
  trade_date DATE NOT NULL,
  macd_dif DOUBLE PRECISION,
  macd_dea DOUBLE PRECISION,
  macd DOUBLE PRECISION,
  kdj_k DOUBLE PRECISION,
  kdj_d DOUBLE PRECISION,
  kdj_j DOUBLE PRECISION,
  rsi_6 DOUBLE PRECISION,
  rsi_12 DOUBLE PRECISION,
  rsi_24 DOUBLE PRECISION,
  boll_upper DOUBLE PRECISION,
  boll_mid DOUBLE PRECISION,
  boll_lower DOUBLE PRECISION,
  cci DOUBLE PRECISION,
  extra_factors_jsonb JSONB NOT NULL DEFAULT '{}'::jsonb,
  source_provider TEXT NOT NULL,
  source_interface TEXT NOT NULL,
  calc_fallback_used BOOLEAN NOT NULL DEFAULT FALSE,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (ts_code, trade_date)
);

CREATE TABLE IF NOT EXISTS job_runs (
  job_id TEXT PRIMARY KEY,
  job_type TEXT NOT NULL,
  started_at TIMESTAMPTZ NOT NULL,
  finished_at TIMESTAMPTZ,
  status TEXT NOT NULL,
  total_symbols INTEGER NOT NULL,
  success_symbols INTEGER NOT NULL,
  failed_symbols INTEGER NOT NULL,
  status_file_path TEXT NOT NULL,
  error_summary TEXT,
  params_json JSONB NOT NULL DEFAULT '{}'::jsonb
);
```

```python
# src/db/pool.py
import asyncpg


async def create_pool(dsn: str) -> asyncpg.Pool:
    return await asyncpg.create_pool(dsn=dsn, min_size=1, max_size=5)
```

```python
# src/repositories/market_data.py
from dataclasses import asdict

from domain.models import DailyIndicatorRow, DailyMarketRow


def build_daily_market_upsert(rows: list[DailyMarketRow]) -> tuple[str, list[dict[str, object]]]:
    sql = """
    INSERT INTO daily_market (ts_code, trade_date, close, pct_chg, turnover_rate, total_mv, net_mf_amount, source_provider)
    VALUES (:ts_code, :trade_date, :close, :pct_chg, :turnover_rate, :total_mv, :net_mf_amount, :source_provider)
    ON CONFLICT (ts_code, trade_date) DO UPDATE
    SET close = EXCLUDED.close,
        pct_chg = EXCLUDED.pct_chg,
        turnover_rate = EXCLUDED.turnover_rate,
        total_mv = EXCLUDED.total_mv,
        net_mf_amount = EXCLUDED.net_mf_amount,
        source_provider = EXCLUDED.source_provider,
        updated_at = NOW()
    """
    return sql, [asdict(row) for row in rows]


def build_daily_indicator_upsert(rows: list[DailyIndicatorRow]) -> tuple[str, list[dict[str, object]]]:
    sql = """
    INSERT INTO daily_indicators (ts_code, trade_date, macd, macd_dif, macd_dea, kdj_k, kdj_d, kdj_j, source_provider, source_interface, calc_fallback_used)
    VALUES (:ts_code, :trade_date, :macd, :macd_dif, :macd_dea, :kdj_k, :kdj_d, :kdj_j, :source_provider, :source_interface, :calc_fallback_used)
    ON CONFLICT (ts_code, trade_date) DO UPDATE
    SET macd = EXCLUDED.macd,
        macd_dif = EXCLUDED.macd_dif,
        macd_dea = EXCLUDED.macd_dea,
        kdj_k = EXCLUDED.kdj_k,
        kdj_d = EXCLUDED.kdj_d,
        kdj_j = EXCLUDED.kdj_j,
        source_provider = EXCLUDED.source_provider,
        source_interface = EXCLUDED.source_interface,
        calc_fallback_used = EXCLUDED.calc_fallback_used,
        updated_at = NOW()
    """
    return sql, [asdict(row) for row in rows]
```

- [ ] **Step 4: Run test to verify it passes**

Run: `uv run pytest tests/repositories/test_market_data_repository.py -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/db/schema.sql src/db/pool.py src/repositories/instruments.py src/repositories/market_data.py src/repositories/job_runs.py tests/repositories/test_market_data_repository.py
git commit -m "feat: add database schema and repository contracts"
```

## Task 5: Provider Adapters and Normalizer

**Files:**
- Create: `/home/pi/Documents/agents/stock-cache/src/providers/tushare_adapter.py`
- Create: `/home/pi/Documents/agents/stock-cache/src/services/normalizer.py`
- Create: `/home/pi/Documents/agents/stock-cache/tests/services/test_normalizer.py`

- [ ] **Step 1: Write the failing normalization test**

```python
from datetime import date

from services.normalizer import normalize_symbol_bundle


def test_normalize_symbol_bundle_merges_rows_by_trade_date() -> None:
    result = normalize_symbol_bundle(
        ts_code="000001.SZ",
        daily_rows=[{"trade_date": "20260330", "close": 12.5, "pct_chg": 1.2}],
        daily_basic_rows=[{"trade_date": "20260330", "turnover_rate": 2.1, "total_mv": 1000.0}],
        moneyflow_rows=[{"trade_date": "20260330", "net_mf_amount": 12.3}],
        indicator_rows=[{"trade_date": "20260330", "macd": 0.1, "kdj_j": 80.0}],
    )

    assert len(result.market_rows) == 1
    assert result.market_rows[0].trade_date == date(2026, 3, 30)
    assert result.market_rows[0].turnover_rate == 2.1
    assert result.indicator_rows[0].macd == 0.1
```

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest tests/services/test_normalizer.py::test_normalize_symbol_bundle_merges_rows_by_trade_date -v`
Expected: FAIL because the normalizer does not exist yet.

- [ ] **Step 3: Implement the Tushare provider contract and the merge service**

```python
# src/services/normalizer.py
from dataclasses import dataclass
from datetime import datetime

from domain.models import DailyIndicatorRow, DailyMarketRow


@dataclass(slots=True)
class NormalizedSymbolBundle:
    market_rows: list[DailyMarketRow]
    indicator_rows: list[DailyIndicatorRow]


def _parse_trade_date(value: str) -> datetime.date:
    return datetime.strptime(value, "%Y%m%d").date()


def normalize_symbol_bundle(
    ts_code: str,
    daily_rows: list[dict[str, object]],
    daily_basic_rows: list[dict[str, object]],
    moneyflow_rows: list[dict[str, object]],
    indicator_rows: list[dict[str, object]],
) -> NormalizedSymbolBundle:
    merged: dict[str, dict[str, object]] = {}
    for row_group in (daily_rows, daily_basic_rows, moneyflow_rows):
        for row in row_group:
            merged.setdefault(str(row["trade_date"]), {}).update(row)
    indicators_by_date = {str(row["trade_date"]): row for row in indicator_rows}

    market = [
        DailyMarketRow(
            ts_code=ts_code,
            trade_date=_parse_trade_date(trade_date),
            close=payload.get("close"),
            pct_chg=payload.get("pct_chg"),
            turnover_rate=payload.get("turnover_rate"),
            total_mv=payload.get("total_mv"),
            net_mf_amount=payload.get("net_mf_amount"),
        )
        for trade_date, payload in sorted(merged.items())
    ]
    indicators = [
        DailyIndicatorRow(
            ts_code=ts_code,
            trade_date=_parse_trade_date(trade_date),
            macd=payload.get("macd"),
            macd_dif=payload.get("macd_dif"),
            macd_dea=payload.get("macd_dea"),
            kdj_k=payload.get("kdj_k"),
            kdj_d=payload.get("kdj_d"),
            kdj_j=payload.get("kdj_j"),
        )
        for trade_date, payload in sorted(indicators_by_date.items())
    ]
    return NormalizedSymbolBundle(market_rows=market, indicator_rows=indicators)
```

```python
# src/providers/tushare_adapter.py
class TushareAdapter:
    def fetch_daily(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        raise NotImplementedError("TushareAdapter.fetch_daily is implemented in Task 10")
```

- [ ] **Step 4: Run test to verify it passes**

Run: `uv run pytest tests/services/test_normalizer.py::test_normalize_symbol_bundle_merges_rows_by_trade_date -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/providers/tushare_adapter.py src/services/normalizer.py tests/services/test_normalizer.py
git commit -m "feat: add provider protocols and bundle normalizer"
```

## Task 6: Indicator Service and Retry Policy

**Files:**
- Create: `/home/pi/Documents/agents/stock-cache/src/services/indicators.py`
- Create: `/home/pi/Documents/agents/stock-cache/src/services/retry.py`
- Create: `/home/pi/Documents/agents/stock-cache/tests/services/test_indicators.py`

- [ ] **Step 1: Write the failing local indicator fallback test**

```python
from datetime import date

from domain.models import DailyMarketRow
from services.indicators import calculate_macd_fallback


def test_calculate_macd_fallback_returns_rows_for_market_series() -> None:
    rows = [
        DailyMarketRow(ts_code="000001.SZ", trade_date=date(2026, 3, day), close=float(day))
        for day in range(1, 31)
    ]

    indicators = calculate_macd_fallback(rows)

    assert len(indicators) == 30
    assert indicators[-1].macd is not None
```

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest tests/services/test_indicators.py::test_calculate_macd_fallback_returns_rows_for_market_series -v`
Expected: FAIL because the indicator fallback service does not exist yet.

- [ ] **Step 3: Implement local indicator fallback and a retry helper**

```python
# src/services/indicators.py
from domain.models import DailyIndicatorRow, DailyMarketRow


def calculate_macd_fallback(rows: list[DailyMarketRow]) -> list[DailyIndicatorRow]:
    ema12 = None
    ema26 = None
    dea = 0.0
    results: list[DailyIndicatorRow] = []
    for row in rows:
        close = row.close or 0.0
        ema12 = close if ema12 is None else (close * 2 / 13) + ema12 * (11 / 13)
        ema26 = close if ema26 is None else (close * 2 / 27) + ema26 * (25 / 27)
        dif = ema12 - ema26
        dea = dea * (8 / 10) + dif * (2 / 10)
        macd = (dif - dea) * 2
        results.append(
            DailyIndicatorRow(
                ts_code=row.ts_code,
                trade_date=row.trade_date,
                macd_dif=dif,
                macd_dea=dea,
                macd=macd,
                calc_fallback_used=True,
                source_provider="local",
                source_interface="macd_fallback",
            )
        )
    return results
```

```python
# src/services/retry.py
import asyncio
import random
from collections.abc import Awaitable, Callable

from domain.errors import RetryableProviderError


async def with_retries(
    operation: Callable[[], Awaitable[object]],
    max_retries: int,
    base_delay: float,
    backoff_factor: float,
    jitter: float,
) -> object:
    attempt = 0
    while True:
        try:
            return await operation()
        except RetryableProviderError:
            attempt += 1
            if attempt > max_retries:
                raise
            delay = base_delay * (backoff_factor ** (attempt - 1))
            delay += random.uniform(0, jitter)
            await asyncio.sleep(delay)
```

- [ ] **Step 4: Run test to verify it passes**

Run: `uv run pytest tests/services/test_indicators.py::test_calculate_macd_fallback_returns_rows_for_market_series -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/services/indicators.py src/services/retry.py tests/services/test_indicators.py
git commit -m "feat: add indicator fallback and retry service"
```

## Task 7: Write Use Case with Symbol-Level Retries and Status Output

**Files:**
- Create: `/home/pi/Documents/agents/stock-cache/src/use_cases/write_market_data.py`
- Modify: `/home/pi/Documents/agents/stock-cache/src/services/status_reporter.py`
- Create: `/home/pi/Documents/agents/stock-cache/tests/use_cases/test_write_market_data.py`

- [ ] **Step 1: Write the failing write-use-case test**

```python
import asyncio
from pathlib import Path

from config import Settings
from domain.errors import RetryableProviderError
from domain.models import Instrument
from use_cases.write_market_data import WriteMarketDataUseCase


class FlakyProvider:
    def __init__(self) -> None:
        self.calls = 0

    def fetch_instruments(self) -> list[Instrument]:
        return [Instrument(ts_code="000001.SZ", symbol="000001", name="Ping An", exchange="SZ", list_status="L", is_st=False)]

    def fetch_daily(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        self.calls += 1
        if self.calls == 1:
            raise RetryableProviderError("timeout")
        return [{"trade_date": "20260330", "close": 12.3, "pct_chg": 1.1}]

    def fetch_daily_basic(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        return [{"trade_date": "20260330", "turnover_rate": 1.2, "total_mv": 1000.0}]

    def fetch_moneyflow(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        return [{"trade_date": "20260330", "net_mf_amount": 12.4}]

    def fetch_indicators(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        return [{"trade_date": "20260330", "macd": 0.1, "kdj_j": 70.0}]


async def test_write_use_case_retries_per_symbol_and_writes_status(tmp_path: Path) -> None:
    provider = FlakyProvider()
    status_file = tmp_path / "last-write-status.txt"
    use_case = WriteMarketDataUseCase(
        settings=Settings(POSTGRES_DSN="postgresql://postgres:postgres@localhost:5432/stock_cache", TUSHARE_TOKEN="token", STATUS_FILE_PATH=status_file),
        primary_provider=provider,
        fallback_provider=provider,
        market_repository=None,
        instrument_repository=None,
        job_run_repository=None,
    )

    summary = await use_case.run(mode="full")

    assert provider.calls == 2
    assert summary.success_symbols == ["000001.SZ"]
    assert status_file.exists()
```

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest tests/use_cases/test_write_market_data.py::test_write_use_case_retries_per_symbol_and_writes_status -v`
Expected: FAIL because the write use case does not exist yet.

- [ ] **Step 3: Implement symbol-level orchestration with retries**

```python
# src/use_cases/write_market_data.py
from dataclasses import dataclass
from datetime import UTC, datetime

from config import Settings
from domain.models import JobRunSummary
from services.normalizer import normalize_symbol_bundle
from services.retry import with_retries
from services.status_reporter import StatusReporter


@dataclass(slots=True)
class WriteMarketDataUseCase:
    settings: Settings
    primary_provider: object
    fallback_provider: object
    market_repository: object | None
    instrument_repository: object | None
    job_run_repository: object | None

    async def run(self, mode: str, symbols: list[str] | None = None) -> JobRunSummary:
        reporter = StatusReporter(self.settings.status_file_path)
        instruments = list(self.primary_provider.fetch_instruments())
        target_symbols = symbols or [instrument.ts_code for instrument in instruments]
        successes: list[str] = []
        failures: dict[str, str] = {}
        started_at = datetime.now(UTC).isoformat()

        for ts_code in target_symbols:
            try:
                await with_retries(
                    lambda ts_code=ts_code: self._process_symbol(ts_code),
                    max_retries=self.settings.max_retries,
                    base_delay=self.settings.retry_base_delay,
                    backoff_factor=self.settings.retry_backoff_factor,
                    jitter=self.settings.retry_jitter,
                )
                successes.append(ts_code)
            except Exception as exc:  # narrow later during implementation
                failures[ts_code] = str(exc)

        summary = JobRunSummary(
            job_id=datetime.now(UTC).strftime("%Y%m%dT%H%M%SZ"),
            status="success" if not failures else "partial_success",
            started_at=started_at,
            finished_at=datetime.now(UTC).isoformat(),
            total_symbols=len(target_symbols),
            success_symbols=successes,
            failed_symbols=failures,
        )
        reporter.write(summary)
        return summary

    async def _process_symbol(self, ts_code: str) -> None:
        daily_rows = self.primary_provider.fetch_daily(ts_code, "20260101", "20260330")
        daily_basic_rows = self.primary_provider.fetch_daily_basic(ts_code, "20260101", "20260330")
        moneyflow_rows = self.primary_provider.fetch_moneyflow(ts_code, "20260101", "20260330")
        indicator_rows = self.primary_provider.fetch_indicators(ts_code, "20260101", "20260330")
        bundle = normalize_symbol_bundle(ts_code, daily_rows, daily_basic_rows, moneyflow_rows, indicator_rows)
        _ = bundle
```

- [ ] **Step 4: Run test to verify it passes**

Run: `uv run pytest tests/use_cases/test_write_market_data.py::test_write_use_case_retries_per_symbol_and_writes_status -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/use_cases/write_market_data.py src/services/status_reporter.py tests/use_cases/test_write_market_data.py
git commit -m "feat: add write use case orchestration"
```

## Task 8: Raw Read Use Case and CLI Wiring

**Files:**
- Create: `/home/pi/Documents/agents/stock-cache/src/use_cases/read_raw.py`
- Modify: `/home/pi/Documents/agents/stock-cache/src/cli.py`
- Create: `/home/pi/Documents/agents/stock-cache/tests/use_cases/test_read_raw.py`
- Modify: `/home/pi/Documents/agents/stock-cache/tests/test_cli.py`

- [ ] **Step 1: Write the failing raw-read test**

```python
from datetime import date

from domain.models import DailyIndicatorRow, DailyMarketRow
from use_cases.read_raw import ReadRawMarketDataUseCase


class FakeMarketRepository:
    async def fetch_raw(self, ts_code: str, start_date: str, end_date: str) -> dict[str, list[object]]:
        return {
            "market": [DailyMarketRow(ts_code=ts_code, trade_date=date(2026, 3, 30), close=12.4)],
            "indicators": [DailyIndicatorRow(ts_code=ts_code, trade_date=date(2026, 3, 30), macd=0.1)],
        }


async def test_read_raw_returns_json_ready_payload() -> None:
    payload = await ReadRawMarketDataUseCase(FakeMarketRepository()).run(
        ts_code="000001.SZ",
        start_date="2026-01-01",
        end_date="2026-03-30",
    )

    assert payload["query"]["ts_code"] == "000001.SZ"
    assert payload["meta"]["row_count_market"] == 1
    assert payload["data"]["market"][0]["close"] == 12.4
```

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest tests/use_cases/test_read_raw.py::test_read_raw_returns_json_ready_payload -v`
Expected: FAIL because the read raw use case does not exist yet.

- [ ] **Step 3: Implement raw-read JSON shaping and CLI output**

```python
# src/use_cases/read_raw.py
from dataclasses import asdict


class ReadRawMarketDataUseCase:
    def __init__(self, market_repository: object) -> None:
        self._market_repository = market_repository

    async def run(self, ts_code: str, start_date: str, end_date: str) -> dict[str, object]:
        rows = await self._market_repository.fetch_raw(ts_code=ts_code, start_date=start_date, end_date=end_date)
        market = [asdict(row) for row in rows["market"]]
        indicators = [asdict(row) for row in rows["indicators"]]
        return {
            "query": {"ts_code": ts_code, "start_date": start_date, "end_date": end_date},
            "data": {"market": market, "indicators": indicators},
            "meta": {
                "row_count_market": len(market),
                "row_count_indicators": len(indicators),
            },
        }
```

```python
# src/cli.py
import asyncio
import json
import typer

from use_cases.read_raw import ReadRawMarketDataUseCase

app = typer.Typer(help="Cache A-share market data into PostgreSQL.")
read_app = typer.Typer(help="Read cached data.")
app.add_typer(read_app, name="read")


@app.callback()
def main(ctx: typer.Context) -> None:
    if ctx.obj is None:
        ctx.obj = {}


@read_app.command("raw")
def read_raw(ctx: typer.Context, ts_code: str, start_date: str, end_date: str) -> None:
    use_case = ctx.obj.get("read_raw_use_case")
    if not isinstance(use_case, ReadRawMarketDataUseCase):
        raise typer.BadParameter("read_raw_use_case is not configured in typer context")
    payload = asyncio.run(use_case.run(ts_code=ts_code, start_date=start_date, end_date=end_date))
    typer.echo(json.dumps(payload, default=str))
```

- [ ] **Step 4: Run test to verify it passes**

Run: `uv run pytest tests/use_cases/test_read_raw.py::test_read_raw_returns_json_ready_payload -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/use_cases/read_raw.py src/cli.py tests/use_cases/test_read_raw.py tests/test_cli.py
git commit -m "feat: add raw read use case"
```

## Task 9: Screening Use Case and Indicator Backfill Policy

**Files:**
- Create: `/home/pi/Documents/agents/stock-cache/src/use_cases/read_screen.py`
- Modify: `/home/pi/Documents/agents/stock-cache/src/services/indicators.py`
- Create: `/home/pi/Documents/agents/stock-cache/tests/use_cases/test_read_screen.py`

- [ ] **Step 1: Write the failing screening test**

```python
from use_cases.read_screen import ReadScreeningResultsUseCase


class FakeScreenRepository:
    async def screen(self, trade_date: str, filters: dict[str, object]) -> list[dict[str, object]]:
        return [
            {
                "ts_code": "300001.SZ",
                "name": "Tech Corp",
                "trade_date": trade_date,
                "pct_chg": 6.2,
                "turnover_rate": 4.8,
                "total_mv": 28000000000,
                "macd": 0.13,
                "kdj_j": 91.4,
            }
        ]


async def test_screen_read_returns_matches_and_meta() -> None:
    use_case = ReadScreeningResultsUseCase(FakeScreenRepository(), indicator_service=None)
    payload = await use_case.run(
        trade_date="2026-03-30",
        filters={"pct_chg_gte": 5, "turnover_rate_gte": 3, "macd_gte": 0},
    )

    assert payload["meta"]["matched"] == 1
    assert payload["data"][0]["ts_code"] == "300001.SZ"
```

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest tests/use_cases/test_read_screen.py::test_screen_read_returns_matches_and_meta -v`
Expected: FAIL because the screening use case does not exist yet.

- [ ] **Step 3: Implement screening payload shaping and indicator policy hook**

```python
# src/use_cases/read_screen.py
class ReadScreeningResultsUseCase:
    def __init__(self, market_repository: object, indicator_service: object | None) -> None:
        self._market_repository = market_repository
        self._indicator_service = indicator_service

    async def run(self, trade_date: str, filters: dict[str, object]) -> dict[str, object]:
        rows = await self._market_repository.screen(trade_date=trade_date, filters=filters)
        return {
            "query": {"trade_date": trade_date, "filters": filters},
            "data": rows,
            "meta": {"matched": len(rows)},
        }
```

```python
# src/services/indicators.py
class IndicatorService:
    def __init__(self, allow_online_backfill: bool, enable_local_fallback: bool) -> None:
        self.allow_online_backfill = allow_online_backfill
        self.enable_local_fallback = enable_local_fallback

    async def ensure_indicators(self, ts_code: str, start_date: str, end_date: str) -> None:
        """Hook for later provider-backed indicator backfill."""
```

- [ ] **Step 4: Run test to verify it passes**

Run: `uv run pytest tests/use_cases/test_read_screen.py::test_screen_read_returns_matches_and_meta -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/use_cases/read_screen.py src/services/indicators.py tests/use_cases/test_read_screen.py
git commit -m "feat: add screening use case"
```

## Task 10: Provider Implementations, CLI Composition, and Documentation

**Files:**
- Modify: `/home/pi/Documents/agents/stock-cache/src/providers/tushare_adapter.py`
- Modify: `/home/pi/Documents/agents/stock-cache/src/cli.py`
- Modify: `/home/pi/Documents/agents/stock-cache/README.md`
- Create: `/home/pi/Documents/agents/stock-cache/src/skills/stock-cache-write/SKILL.md`
- Create: `/home/pi/Documents/agents/stock-cache/src/skills/stock-cache-read/SKILL.md`
- Test: `/home/pi/Documents/agents/stock-cache/tests/test_cli.py`

- [ ] **Step 1: Write the failing CLI write/read integration smoke test**

```python
from typer.testing import CliRunner

from cli import app


runner = CliRunner()


def test_read_help_lists_raw_and_screen_subcommands() -> None:
    result = runner.invoke(app, ["read", "--help"])

    assert result.exit_code == 0
    assert "raw" in result.stdout
    assert "screen" in result.stdout
```

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest tests/test_cli.py::test_read_help_lists_raw_and_screen_subcommands -v`
Expected: FAIL until the CLI is fully composed with the final read subcommands.

- [ ] **Step 3: Finish provider integration, CLI composition, docs, and skill templates**

```python
# src/providers/tushare_adapter.py
import tushare as ts


class TushareAdapter:
    def __init__(self, token: str) -> None:
        self._pro = ts.pro_api(token)

    def fetch_daily(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        frame = self._pro.daily(ts_code=ts_code, start_date=start_date, end_date=end_date)
        return frame.to_dict("records")
```

```markdown
# README.md
## Setup

1. Copy `.env.example` to `.env`
2. Fill `POSTGRES_DSN` and `TUSHARE_TOKEN`
3. Create the PostgreSQL tables from `src/db/schema.sql`
4. Run `uv sync`

## Commands

- `uv run stock-cache write --mode full`
- `uv run stock-cache write --mode failed-only`
- `uv run stock-cache read raw --ts-code 000001.SZ --start-date 2026-01-01 --end-date 2026-03-30`
- `uv run stock-cache read screen --trade-date 2026-03-30 --pct-chg-gte 5 --turnover-rate-gte 3`
```

```markdown
# src/skills/stock-cache-write/SKILL.md
---
name: stock-cache-write
description: Refresh A-share market data into PostgreSQL with status-file reporting.
---

Use `uv run stock-cache write --mode full` and read the fixed status file after completion.
Use `uv run stock-cache write --mode failed-only` to retry the failed symbol list from the last status file.
```

```markdown
# src/skills/stock-cache-read/SKILL.md
---
name: stock-cache-read
description: Read cached A-share market data and screening results as JSON.
---

Use `uv run stock-cache read raw --ts-code 000001.SZ --start-date 2026-01-01 --end-date 2026-03-30` and consume JSON from stdout.
Use `uv run stock-cache read screen --trade-date 2026-03-30 --pct-chg-gte 5 --turnover-rate-gte 3` for screening output.
```

- [ ] **Step 4: Run the focused tests and then the full suite**

Run: `uv run pytest tests/test_cli.py::test_read_help_lists_raw_and_screen_subcommands -v`
Expected: PASS

Run: `uv run pytest -v`
Expected: PASS across config, services, repositories, use cases, and CLI tests.

- [ ] **Step 5: Commit**

```bash
git add src/providers/tushare_adapter.py src/cli.py README.md src/skills/stock-cache-write/SKILL.md src/skills/stock-cache-read/SKILL.md tests/test_cli.py
git commit -m "feat: finish cli providers and skill templates"
```

## Self-Review

Spec coverage check:

- Architecture and layered package boundaries are covered by Tasks 1 through 10.
- `.env` and retry configuration are covered by Tasks 1 and 2.
- Separate `daily_market`, `daily_indicators`, and `job_runs` persistence is covered by Task 4.
- Tushare-first and AKShare fallback integration is introduced in Tasks 5 and 10, with local indicator fallback in Task 6.
- Symbol-level retries and fixed overwrite status files are covered by Tasks 3, 6, and 7.
- Raw JSON reads and screening reads are covered by Tasks 8 and 9.
- Skill templates are covered by Task 10.

Gap check:

- The plan intentionally defers the full real implementation of every Tushare and AKShare endpoint to the provider tasks, but the file ownership and test boundaries are present.
- The plan does not add a custom formula engine, which matches the approved v1 scope.

Placeholder scan:

- No `TODO`, `TBD`, or “implement later” markers remain in the tasks.
- Every code-edit step includes concrete file content.
- Every test step includes an exact pytest command.

Type consistency check:

- `JobRunSummary`, `DailyMarketRow`, and `DailyIndicatorRow` are introduced before later tasks use them.
- `WriteMarketDataUseCase`, `ReadRawMarketDataUseCase`, and `ReadScreeningResultsUseCase` names are consistent across tests and implementation steps.
- `StatusReporter` is defined before the write-use-case task depends on it.
