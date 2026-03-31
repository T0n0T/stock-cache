# Write Memory Optimization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate the full-window memory spike in `write --mode full` by streaming each trade date through normalization and persistence immediately, while also chunking repository upserts with `WRITE_BATCH_SIZE`.

**Architecture:** Keep orchestration in `src/use_cases/write_market_data.py` by processing one trade date at a time and removing the long-lived cross-date accumulator lists. Keep batching in `src/repositories/market_data.py` so persistence concerns stay in the repository and both `full` and `single` modes benefit from bounded `executemany` payloads.

**Tech Stack:** Python 3.13, Typer CLI, asyncpg, Tushare adapter, pytest

---

### Task 1: Add failing tests for streaming full-mode writes

**Files:**
- Modify: `tests/use_cases/test_write_market_data.py:122-412`
- Modify: `src/use_cases/write_market_data.py:89-159`

- [ ] **Step 1: Write the failing tests**

```python
class RecordingMarketRepository:
    def __init__(self) -> None:
        self.market_rows: list[object] = []
        self.indicator_rows: list[object] = []
        self.market_write_batches: list[list[object]] = []
        self.indicator_write_batches: list[list[object]] = []

    async def upsert_daily_market(self, rows: list[object]) -> None:
        self.market_rows = rows
        self.market_write_batches.append(list(rows))

    async def upsert_daily_indicators(self, rows: list[object]) -> None:
        self.indicator_rows = rows
        self.indicator_write_batches.append(list(rows))


@pytest.mark.asyncio
async def test_write_use_case_full_mode_persists_each_trade_date_immediately(tmp_path: Path) -> None:
    provider = FlakyProvider()
    repository = RecordingMarketRepository()
    status_file = tmp_path / "last-write-status.txt"
    use_case = WriteMarketDataUseCase(
        settings=Settings(
            POSTGRES_DSN="postgresql://postgres:postgres@localhost:5432/stock_cache",
            TUSHARE_TOKEN="token",
            STATUS_FILE_PATH=status_file,
        ),
        primary_provider=provider,
        market_repository=repository,
        instrument_repository=None,
        job_run_repository=None,
    )

    summary = await use_case.run(mode="full")

    assert summary.status == "success"
    assert len(repository.market_write_batches) == 5
    assert len(repository.indicator_write_batches) == 5
    assert [rows[0].trade_date.isoformat() for rows in repository.market_write_batches] == [
        "2026-03-30",
        "2026-03-27",
        "2026-03-26",
        "2026-03-25",
        "2026-03-24",
    ]


class OneDayFailingRepository(RecordingMarketRepository):
    async def upsert_daily_market(self, rows: list[object]) -> None:
        self.market_rows = rows
        self.market_write_batches.append(list(rows))
        if rows[0].trade_date.isoformat() == "2026-03-27":
            raise RuntimeError("db write failed")


@pytest.mark.asyncio
async def test_write_use_case_full_mode_continues_after_trade_date_persist_failure(tmp_path: Path) -> None:
    provider = FlakyProvider()
    repository = OneDayFailingRepository()
    status_file = tmp_path / "last-write-status.txt"
    use_case = WriteMarketDataUseCase(
        settings=Settings(
            POSTGRES_DSN="postgresql://postgres:postgres@localhost:5432/stock_cache",
            TUSHARE_TOKEN="token",
            STATUS_FILE_PATH=status_file,
        ),
        primary_provider=provider,
        market_repository=repository,
        instrument_repository=None,
        job_run_repository=None,
    )

    summary = await use_case.run(mode="full")

    assert summary.status == "partial_success"
    assert summary.failed_symbols["__trade_date__:20260327"] == "db write failed"
    assert [rows[0].trade_date.isoformat() for rows in repository.market_write_batches] == [
        "2026-03-30",
        "2026-03-27",
        "2026-03-26",
        "2026-03-25",
        "2026-03-24",
    ]
```

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest tests/use_cases/test_write_market_data.py -k "persists_each_trade_date_immediately or continues_after_trade_date_persist_failure" -v`
Expected: FAIL because `full` mode currently accumulates all trade dates and writes only once after the loop.

- [ ] **Step 3: Write minimal implementation**

```python
        successes: list[str] = []
        failures: dict[str, str] = {}
        for index, trade_date in enumerate(trade_dates, start=1):
            self._emit_progress(progress, f"syncing trade date {trade_date} ({index}/{len(trade_dates)})")
            try:
                payload = await with_retries(
                    lambda trade_date=trade_date: self._fetch_trade_date_payload(trade_date),
                    max_retries=self.settings.max_retries,
                    base_delay=self.settings.retry_base_delay,
                    backoff_factor=self.settings.retry_backoff_factor,
                    jitter=self.settings.retry_jitter,
                )
                bundle = normalize_market_batches(
                    daily_rows=payload[0],
                    daily_basic_rows=payload[1],
                    moneyflow_rows=payload[2],
                    adj_factor_rows=payload[3],
                    limit_rows=payload[4],
                    suspend_rows=payload[5],
                    indicator_rows=payload[6],
                    target_symbols=set(target_symbols),
                )
                if self.market_repository is not None:
                    self._emit_progress(
                        progress,
                        f"persisting {len(bundle.market_rows)} market row(s) and {len(bundle.indicator_rows)} indicator row(s)",
                    )
                    await self.market_repository.upsert_daily_market(bundle.market_rows)
                    await self.market_repository.upsert_daily_indicators(bundle.indicator_rows)
            except StockCacheError as exc:
                failures[f"__trade_date__:{trade_date}"] = str(exc)
                self._emit_progress(progress, f"trade date {trade_date} failed: {exc}")
            except Exception as exc:
                failures[f"__trade_date__:{trade_date}"] = str(exc)
                self._emit_progress(progress, f"trade date {trade_date} failed: {exc}")
```

- [ ] **Step 4: Run test to verify it passes**

Run: `uv run pytest tests/use_cases/test_write_market_data.py -k "persists_each_trade_date_immediately or continues_after_trade_date_persist_failure" -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add tests/use_cases/test_write_market_data.py src/use_cases/write_market_data.py
git commit -m "refactor: stream full-mode writes by trade date"
```

### Task 2: Add failing tests for repository chunked upserts

**Files:**
- Modify: `tests/repositories/test_market_data_repository.py:1-267`
- Modify: `src/repositories/market_data.py:41-57`

- [ ] **Step 1: Write the failing tests**

```python
class _RecordingExecutemanyConnection:
    def __init__(self) -> None:
        self.executemany_calls: list[tuple[str, list[tuple[object, ...]]]] = []

    async def executemany(self, sql: str, values: list[tuple[object, ...]]) -> None:
        self.executemany_calls.append((sql, values))


@pytest.mark.asyncio
async def test_upsert_daily_market_splits_rows_into_write_batches() -> None:
    connection = _RecordingExecutemanyConnection()
    repository = MarketDataRepository(_FakePool(connection), write_batch_size=2)
    rows = [
        DailyMarketRow(ts_code="000001.SZ", trade_date=date(2026, 3, 30)),
        DailyMarketRow(ts_code="000001.SZ", trade_date=date(2026, 3, 27)),
        DailyMarketRow(ts_code="000001.SZ", trade_date=date(2026, 3, 26)),
    ]

    await repository.upsert_daily_market(rows)

    assert [len(values) for _, values in connection.executemany_calls] == [2, 1]


@pytest.mark.asyncio
async def test_upsert_daily_indicators_ignores_invalid_batch_size_by_falling_back_to_single_batch() -> None:
    connection = _RecordingExecutemanyConnection()
    repository = MarketDataRepository(_FakePool(connection), write_batch_size=0)
    rows = [
        DailyIndicatorRow(ts_code="000001.SZ", trade_date=date(2026, 3, 30)),
        DailyIndicatorRow(ts_code="000001.SZ", trade_date=date(2026, 3, 27)),
    ]

    await repository.upsert_daily_indicators(rows)

    assert [len(values) for _, values in connection.executemany_calls] == [2]
```

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest tests/repositories/test_market_data_repository.py -k "write_batches or invalid_batch_size" -v`
Expected: FAIL because the repository constructor does not currently accept `write_batch_size` and each upsert uses a single `executemany`.

- [ ] **Step 3: Write minimal implementation**

```python
class MarketDataRepository:
    def __init__(self, pool: asyncpg.Pool, write_batch_size: int = 500) -> None:
        self._pool = pool
        self._write_batch_size = write_batch_size if write_batch_size > 0 else 500

    async def upsert_daily_market(self, rows: list[DailyMarketRow]) -> None:
        if not rows:
            return
        sql, _ = build_daily_market_upsert(rows[:1])
        async with self._pool.acquire() as connection:
            for chunk in _chunk_rows(rows, self._write_batch_size):
                _, values = build_daily_market_upsert(chunk)
                await connection.executemany(sql, values)

    async def upsert_daily_indicators(self, rows: list[DailyIndicatorRow]) -> None:
        if not rows:
            return
        sql, _ = build_daily_indicator_upsert(rows[:1])
        async with self._pool.acquire() as connection:
            for chunk in _chunk_rows(rows, self._write_batch_size):
                _, values = build_daily_indicator_upsert(chunk)
                await connection.executemany(sql, values)


def _chunk_rows[T](rows: list[T], batch_size: int) -> list[list[T]]:
    return [rows[index:index + batch_size] for index in range(0, len(rows), batch_size)]
```

- [ ] **Step 4: Run test to verify it passes**

Run: `uv run pytest tests/repositories/test_market_data_repository.py -k "write_batches or invalid_batch_size" -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add tests/repositories/test_market_data_repository.py src/repositories/market_data.py
git commit -m "refactor: chunk market data upserts"
```

### Task 3: Wire the configured batch size into live repository construction

**Files:**
- Modify: `src/cli.py`
- Modify: `tests/test_cli.py`

- [ ] **Step 1: Write the failing test**

```python
def test_run_write_builds_market_repository_with_configured_batch_size(monkeypatch, sample_dsn: str, tmp_path) -> None:
    captured: dict[str, object] = {}

    class FakeMarketDataRepository:
        def __init__(self, pool: object, write_batch_size: int) -> None:
            captured["pool"] = pool
            captured["write_batch_size"] = write_batch_size

    monkeypatch.setattr("cli.MarketDataRepository", FakeMarketDataRepository)
    monkeypatch.setenv("POSTGRES_DSN", sample_dsn)
    monkeypatch.setenv("TUSHARE_TOKEN", "token")
    monkeypatch.setenv("WRITE_BATCH_SIZE", "321")
    monkeypatch.setenv("STATUS_FILE_PATH", str(tmp_path / "status.txt"))

    ...

    assert captured["write_batch_size"] == 321
```

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest tests/test_cli.py -k "configured_batch_size" -v`
Expected: FAIL because the CLI currently instantiates `MarketDataRepository` without a batch-size argument.

- [ ] **Step 3: Write minimal implementation**

```python
    market_repository = MarketDataRepository(
        pool,
        write_batch_size=settings.write_batch_size,
    )
```

- [ ] **Step 4: Run test to verify it passes**

Run: `uv run pytest tests/test_cli.py -k "configured_batch_size" -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add tests/test_cli.py src/cli.py
git commit -m "feat: pass configured write batch size to repository"
```

### Task 4: Update write-path docs and run focused verification

**Files:**
- Modify: `README.md:68-86`
- Modify: `README.md:116-179`

- [ ] **Step 1: Update the README**

```markdown
| `WRITE_BATCH_SIZE` | No | `500` | maximum rows per repository upsert batch |

During `write --mode full`, the CLI fetches one trade date at a time, normalizes it immediately, and persists rows in chunks controlled by `WRITE_BATCH_SIZE` to keep memory usage bounded.
```

- [ ] **Step 2: Run focused test verification**

Run: `uv run pytest tests/use_cases/test_write_market_data.py tests/repositories/test_market_data_repository.py tests/test_cli.py -k "persists_each_trade_date_immediately or continues_after_trade_date_persist_failure or write_batches or invalid_batch_size or configured_batch_size" -v`
Expected: PASS

- [ ] **Step 3: Run broader regression verification**

Run: `uv run pytest tests/use_cases/test_write_market_data.py tests/repositories/test_market_data_repository.py tests/test_cli.py -v`
Expected: PASS

- [ ] **Step 4: Run CLI verification**

Run: `uv run stock-cache --help`
Expected: PASS with `write`, `read`, `stats`, and `delete` commands listed.

- [ ] **Step 5: Commit**

```bash
git add README.md
git commit -m "docs: describe streaming writes and batch upserts"
```

### Task 5: Final verification and integration commit

**Files:**
- Modify: `src/use_cases/write_market_data.py`
- Modify: `src/repositories/market_data.py`
- Modify: `src/cli.py`
- Modify: `tests/use_cases/test_write_market_data.py`
- Modify: `tests/repositories/test_market_data_repository.py`
- Modify: `tests/test_cli.py`
- Modify: `README.md`

- [ ] **Step 1: Review the final diff**

```bash
git diff -- src/use_cases/write_market_data.py src/repositories/market_data.py src/cli.py tests/use_cases/test_write_market_data.py tests/repositories/test_market_data_repository.py tests/test_cli.py README.md
```

- [ ] **Step 2: Run the full required verification suite**

Run: `uv run pytest tests/use_cases/test_write_market_data.py tests/repositories/test_market_data_repository.py tests/test_cli.py -v`
Expected: PASS

Run: `uv run stock-cache --help`
Expected: PASS

- [ ] **Step 3: Run an optional live verification if PostgreSQL is available**

Run: `uv run pytest tests/integration/test_write_market_data_live.py -v`
Expected: PASS if a live PostgreSQL instance and valid Tushare credentials are available; otherwise document that this verification was not run.

- [ ] **Step 4: Stage the integrated change**

```bash
git add src/use_cases/write_market_data.py src/repositories/market_data.py src/cli.py tests/use_cases/test_write_market_data.py tests/repositories/test_market_data_repository.py tests/test_cli.py README.md
```

- [ ] **Step 5: Commit**

```bash
git commit -m "refactor: reduce write memory pressure"
```
