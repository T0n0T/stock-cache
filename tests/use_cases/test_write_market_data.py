from datetime import UTC, datetime
from pathlib import Path

import pytest

from config import Settings
from domain.errors import NonRetryableProviderError, RetryableProviderError
from domain.models import Instrument
from use_cases.write_market_data import WriteMarketDataUseCase


class FlakyProvider:
    def __init__(self) -> None:
        self.calls = 0
        self.instrument_fetches = 0
        self.trade_date_requests: list[tuple[str, int]] = []
        self.batch_dates: list[str] = []

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

    async def fetch_daily_by_trade_date(self, trade_date: str) -> list[dict[str, object]]:
        self.batch_dates.append(trade_date)
        self.calls += 1
        if self.calls == 1:
            raise RetryableProviderError("timeout")
        return [{"ts_code": "000001.SZ", "trade_date": trade_date, "close": 12.3, "pct_chg": 1.1}]

    async def fetch_daily_basic_by_trade_date(self, trade_date: str) -> list[dict[str, object]]:
        return [{"ts_code": "000001.SZ", "trade_date": trade_date, "turnover_rate": 1.2, "total_mv": 1000.0}]

    async def fetch_moneyflow_by_trade_date(self, trade_date: str) -> list[dict[str, object]]:
        return [{"ts_code": "000001.SZ", "trade_date": trade_date, "net_mf_amount": 12.4}]

    async def fetch_adj_factor_by_trade_date(self, trade_date: str) -> list[dict[str, object]]:
        return [{"ts_code": "000001.SZ", "trade_date": trade_date, "adj_factor": 123.4}]

    async def fetch_stk_limit_by_trade_date(self, trade_date: str) -> list[dict[str, object]]:
        return [{"ts_code": "000001.SZ", "trade_date": trade_date, "up_limit": 13.5, "down_limit": 11.0}]

    async def fetch_suspend_d_by_trade_date(self, trade_date: str) -> list[dict[str, object]]:
        return []

    async def fetch_indicators_by_trade_date(self, trade_date: str) -> list[dict[str, object]]:
        return [{"ts_code": "000001.SZ", "trade_date": trade_date, "macd": 0.1, "kdj_j": 70.0}]


class AlwaysFailProvider(FlakyProvider):
    async def fetch_daily_by_trade_date(self, trade_date: str) -> list[dict[str, object]]:
        raise NonRetryableProviderError("symbol permanently failed")


class PrimaryFailingProvider(FlakyProvider):
    async def fetch_daily_by_trade_date(self, trade_date: str) -> list[dict[str, object]]:
        raise NonRetryableProviderError("primary provider failed")


class CrashingProvider(FlakyProvider):
    async def fetch_daily_by_trade_date(self, trade_date: str) -> list[dict[str, object]]:
        raise RuntimeError("dns failed")


class StartupFailingProvider(FlakyProvider):
    def fetch_instruments(self) -> list[Instrument]:
        self.instrument_fetches += 1
        raise RetryableProviderError("instrument api unavailable")




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
        market_repository=None,
        instrument_repository=None,
        job_run_repository=None,
    )

    summary = await use_case.run(mode="full")

    assert provider.calls == 6
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
        market_repository=None,
        instrument_repository=None,
        job_run_repository=None,
        now_provider=lambda: datetime(2026, 3, 30, 12, tzinfo=UTC),
    )

    await use_case.run(mode="full")

    assert provider.trade_date_requests == [("20260330", 5)]
    assert provider.batch_dates == ["20260330", "20260330", "20260327", "20260326", "20260325", "20260324"]


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
        market_repository=None,
        instrument_repository=None,
        job_run_repository=None,
    )

    summary = await use_case.run(mode="full")

    assert summary.status == "partial_success"
    assert summary.success_symbols == []
    assert summary.failed_symbols == {
        "__trade_date__:20260330": "symbol permanently failed",
        "__trade_date__:20260327": "symbol permanently failed",
        "__trade_date__:20260326": "symbol permanently failed",
        "__trade_date__:20260325": "symbol permanently failed",
        "__trade_date__:20260324": "symbol permanently failed",
    }
    contents = status_file.read_text(encoding="utf-8")
    assert "failed_count: 5" in contents
    assert "__trade_date__:20260330 | symbol permanently failed" in contents


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
    assert provider.batch_dates == []


@pytest.mark.asyncio
async def test_write_use_case_reports_primary_provider_failure_without_fallback(tmp_path: Path) -> None:
    provider = PrimaryFailingProvider()
    status_file = tmp_path / "last-write-status.txt"
    use_case = WriteMarketDataUseCase(
        settings=Settings(
            POSTGRES_DSN="postgresql://postgres:postgres@localhost:5432/stock_cache",
            TUSHARE_TOKEN="token",
            STATUS_FILE_PATH=status_file,
        ),
        primary_provider=provider,
        market_repository=None,
        instrument_repository=None,
        job_run_repository=None,
    )

    summary = await use_case.run(mode="full")

    assert summary.status == "partial_success"
    assert summary.success_symbols == []
    assert summary.failed_symbols == {
        "__trade_date__:20260330": "primary provider failed",
        "__trade_date__:20260327": "primary provider failed",
        "__trade_date__:20260326": "primary provider failed",
        "__trade_date__:20260325": "primary provider failed",
        "__trade_date__:20260324": "primary provider failed",
    }
    assert provider.batch_dates == []


@pytest.mark.asyncio
async def test_write_use_case_records_failure_and_status_file_for_unexpected_provider_errors(
    tmp_path: Path,
) -> None:
    provider = CrashingProvider()
    status_file = tmp_path / "last-write-status.txt"
    use_case = WriteMarketDataUseCase(
        settings=Settings(
            POSTGRES_DSN="postgresql://postgres:postgres@localhost:5432/stock_cache",
            TUSHARE_TOKEN="token",
            STATUS_FILE_PATH=status_file,
        ),
        primary_provider=provider,
        market_repository=None,
        instrument_repository=None,
        job_run_repository=None,
    )

    summary = await use_case.run(mode="full")

    assert summary.status == "partial_success"
    assert summary.success_symbols == []
    assert summary.failed_symbols == {
        "__trade_date__:20260330": "dns failed",
        "__trade_date__:20260327": "dns failed",
        "__trade_date__:20260326": "dns failed",
        "__trade_date__:20260325": "dns failed",
        "__trade_date__:20260324": "dns failed",
    }
    assert status_file.exists()


@pytest.mark.asyncio
async def test_write_use_case_records_failure_when_startup_provider_call_fails(tmp_path: Path) -> None:
    provider = StartupFailingProvider()
    status_file = tmp_path / "last-write-status.txt"
    use_case = WriteMarketDataUseCase(
        settings=Settings(
            POSTGRES_DSN="postgresql://postgres:postgres@localhost:5432/stock_cache",
            TUSHARE_TOKEN="token",
            STATUS_FILE_PATH=status_file,
        ),
        primary_provider=provider,
        market_repository=None,
        instrument_repository=None,
        job_run_repository=None,
    )

    summary = await use_case.run(mode="full")

    assert summary.status == "failed"
    assert summary.total_symbols == 0
    assert summary.success_symbols == []
    assert summary.failed_symbols == {"__startup__": "instrument api unavailable"}
    assert status_file.exists()
