from datetime import UTC, datetime
from pathlib import Path

import pytest

from stock_cache.config import Settings
from stock_cache.domain.errors import NonRetryableProviderError, RetryableProviderError
from stock_cache.domain.models import Instrument
from stock_cache.use_cases.write_market_data import WriteMarketDataUseCase


class FlakyProvider:
    def __init__(self) -> None:
        self.calls = 0
        self.instrument_fetches = 0
        self.trade_date_requests: list[tuple[str, int]] = []
        self.daily_windows: list[tuple[str, str, str]] = []
        self.daily_basic_windows: list[tuple[str, str, str]] = []
        self.moneyflow_windows: list[tuple[str, str, str]] = []
        self.indicator_windows: list[tuple[str, str, str]] = []

    def fetch_instruments(self) -> list[Instrument]:
        self.instrument_fetches += 1
        return [
            Instrument(
                ts_code="000001.SZ",
                symbol="000001",
                name="Ping An",
                exchange="SZ",
                list_status="L",
                is_st=False,
            )
        ]

    def fetch_recent_trade_dates(self, end_date: str, limit: int) -> list[str]:
        self.trade_date_requests.append((end_date, limit))
        return [
            "20260330",
            "20260327",
            "20260326",
            "20260325",
            "20260324",
        ][:limit]

    def fetch_daily(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        self.daily_windows.append((ts_code, start_date, end_date))
        self.calls += 1
        if self.calls == 1:
            raise RetryableProviderError("timeout")
        return [{"trade_date": "20260330", "close": 12.3, "pct_chg": 1.1}]

    def fetch_daily_basic(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        self.daily_basic_windows.append((ts_code, start_date, end_date))
        return [{"trade_date": "20260330", "turnover_rate": 1.2, "total_mv": 1000.0}]

    def fetch_moneyflow(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        self.moneyflow_windows.append((ts_code, start_date, end_date))
        return [{"trade_date": "20260330", "net_mf_amount": 12.4}]

    def fetch_indicators(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        self.indicator_windows.append((ts_code, start_date, end_date))
        return [{"trade_date": "20260330", "macd": 0.1, "kdj_j": 70.0}]


class AlwaysFailProvider(FlakyProvider):
    def fetch_daily(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        self.daily_windows.append((ts_code, start_date, end_date))
        raise NonRetryableProviderError("symbol permanently failed")


@pytest.mark.asyncio
async def test_write_use_case_retries_per_symbol_and_writes_status(tmp_path: Path) -> None:
    provider = FlakyProvider()
    status_file = tmp_path / "last-write-status.txt"
    use_case = WriteMarketDataUseCase(
        settings=Settings(
            POSTGRES_DSN="postgresql://postgres:postgres@localhost:5432/stock_cache",
            TUSHARE_TOKEN="token",
            STATUS_FILE_PATH=status_file,
        ),
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


@pytest.mark.asyncio
async def test_write_use_case_uses_configured_lookback_window(tmp_path: Path) -> None:
    provider = FlakyProvider()
    status_file = tmp_path / "last-write-status.txt"
    use_case = WriteMarketDataUseCase(
        settings=Settings(
            POSTGRES_DSN="postgresql://postgres:postgres@localhost:5432/stock_cache",
            TUSHARE_TOKEN="token",
            STATUS_FILE_PATH=status_file,
            DEFAULT_LOOKBACK_TRADING_DAYS=5,
        ),
        primary_provider=provider,
        fallback_provider=provider,
        market_repository=None,
        instrument_repository=None,
        job_run_repository=None,
        now_provider=lambda: datetime(2026, 3, 30, 12, tzinfo=UTC),
    )

    await use_case.run(mode="full")

    assert provider.trade_date_requests == [("20260330", 5)]
    assert provider.daily_windows == [("000001.SZ", "20260324", "20260330"), ("000001.SZ", "20260324", "20260330")]
    assert provider.daily_basic_windows == [("000001.SZ", "20260324", "20260330")]
    assert provider.moneyflow_windows == [("000001.SZ", "20260324", "20260330")]
    assert provider.indicator_windows == [("000001.SZ", "20260324", "20260330")]


@pytest.mark.asyncio
async def test_write_use_case_reports_failed_symbols_in_summary_and_status_file(tmp_path: Path) -> None:
    provider = AlwaysFailProvider()
    status_file = tmp_path / "last-write-status.txt"
    use_case = WriteMarketDataUseCase(
        settings=Settings(
            POSTGRES_DSN="postgresql://postgres:postgres@localhost:5432/stock_cache",
            TUSHARE_TOKEN="token",
            STATUS_FILE_PATH=status_file,
        ),
        primary_provider=provider,
        fallback_provider=provider,
        market_repository=None,
        instrument_repository=None,
        job_run_repository=None,
    )

    summary = await use_case.run(mode="full")

    assert summary.status == "partial_success"
    assert summary.success_symbols == []
    assert summary.failed_symbols == {"000001.SZ": "symbol permanently failed"}
    contents = status_file.read_text(encoding="utf-8")
    assert "failed_count: 1" in contents
    assert "000001.SZ | symbol permanently failed" in contents


@pytest.mark.asyncio
async def test_write_use_case_with_empty_symbol_list_does_not_expand_to_full_universe(tmp_path: Path) -> None:
    provider = FlakyProvider()
    status_file = tmp_path / "last-write-status.txt"
    use_case = WriteMarketDataUseCase(
        settings=Settings(
            POSTGRES_DSN="postgresql://postgres:postgres@localhost:5432/stock_cache",
            TUSHARE_TOKEN="token",
            STATUS_FILE_PATH=status_file,
        ),
        primary_provider=provider,
        fallback_provider=provider,
        market_repository=None,
        instrument_repository=None,
        job_run_repository=None,
    )

    summary = await use_case.run(mode="full", symbols=[])

    assert summary.total_symbols == 0
    assert summary.success_symbols == []
    assert summary.failed_symbols == {}
    assert provider.instrument_fetches == 0
    assert provider.trade_date_requests == []
    assert provider.daily_windows == []
