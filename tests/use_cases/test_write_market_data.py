from datetime import UTC, datetime
from pathlib import Path

import pytest

from config import Settings
from domain.errors import NonRetryableProviderError, RetryableProviderError
from domain.models import Instrument
from use_cases.write_market_data import WriteDateRange, WriteMarketDataUseCase


class FlakyProvider:
    def __init__(self) -> None:
        self.calls = 0
        self.instrument_fetches = 0
        self.trade_date_requests: list[tuple[str, int]] = []
        self.trade_date_range_requests: list[tuple[str, str]] = []
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

    def fetch_trade_dates_in_range(self, start_date: str, end_date: str) -> list[str]:
        self.trade_date_range_requests.append((start_date, end_date))
        return ["20260102", "20260105", "20260331"]

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

    def fetch_index_daily(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        _ = (ts_code, start_date, end_date)
        return []

    def fetch_sw_daily(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        _ = (ts_code, start_date, end_date)
        return []


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


class SingleSymbolProvider(FlakyProvider):
    def __init__(self) -> None:
        super().__init__()
        self.daily_requests: list[tuple[str, str, str]] = []
        self.daily_basic_requests: list[tuple[str, str, str]] = []
        self.moneyflow_requests: list[tuple[str, str, str]] = []
        self.indicator_requests: list[tuple[str, str, str]] = []

    def fetch_daily(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        self.daily_requests.append((ts_code, start_date, end_date))
        return [
            {"trade_date": "20260327", "close": 11.8, "pct_chg": 0.8},
            {"trade_date": "20260330", "close": 12.3, "pct_chg": 1.1},
        ]

    def fetch_daily_basic(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        self.daily_basic_requests.append((ts_code, start_date, end_date))
        return [{"trade_date": "20260330", "turnover_rate": 1.2, "total_mv": 1000.0}]

    def fetch_moneyflow(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        self.moneyflow_requests.append((ts_code, start_date, end_date))
        return [{"trade_date": "20260330", "net_mf_amount": 12.4}]

    def fetch_indicators(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        self.indicator_requests.append((ts_code, start_date, end_date))
        return [{"trade_date": "20260330", "macd": 0.1, "kdj_j": 70.0}]


class RecordingMarketRepository:
    def __init__(self) -> None:
        self.market_rows: list[object] = []
        self.indicator_rows: list[object] = []

    async def upsert_daily_market(self, rows: list[object]) -> None:
        self.market_rows = rows

    async def upsert_daily_indicators(self, rows: list[object]) -> None:
        self.indicator_rows = rows


class StableTradeDateProvider(FlakyProvider):
    async def fetch_daily_by_trade_date(self, trade_date: str) -> list[dict[str, object]]:
        self.batch_dates.append(trade_date)
        return [{"ts_code": "000001.SZ", "trade_date": trade_date, "close": 12.3, "pct_chg": 1.1}]


class SequencedPersistRepository:
    def __init__(self) -> None:
        self.market_trade_dates_by_call: list[list[str]] = []
        self.indicator_trade_dates_by_call: list[list[str]] = []
        self.index_trade_dates_by_call: list[list[str]] = []
        self.index_codes_by_call: list[list[str]] = []

    async def upsert_daily_market(self, rows: list[object]) -> None:
        self.market_trade_dates_by_call.append([row.trade_date.strftime("%Y%m%d") for row in rows])

    async def upsert_daily_indicators(self, rows: list[object]) -> None:
        self.indicator_trade_dates_by_call.append([row.trade_date.strftime("%Y%m%d") for row in rows])

    async def upsert_daily_index(self, rows: list[object]) -> None:
        self.index_trade_dates_by_call.append([row.trade_date.strftime("%Y%m%d") for row in rows])
        self.index_codes_by_call.append([row.ts_code for row in rows])


class IndexAwareProvider(StableTradeDateProvider):
    def __init__(self) -> None:
        super().__init__()
        self.index_daily_requests: list[tuple[str, str, str]] = []
        self.sw_daily_requests: list[tuple[str, str, str]] = []

    def fetch_index_daily(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        self.index_daily_requests.append((ts_code, start_date, end_date))
        return [
            {"ts_code": ts_code, "trade_date": "20260327", "close": 100.0, "pct_chg": 1.0, "source_daily": "index_daily"},
            {"ts_code": ts_code, "trade_date": "20260330", "close": 101.0, "pct_chg": 1.1, "source_daily": "index_daily"},
        ]

    def fetch_sw_daily(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        self.sw_daily_requests.append((ts_code, start_date, end_date))
        return [
            {
                "ts_code": ts_code,
                "trade_date": "20260327",
                "name": "SW Name",
                "close": 200.0,
                "pct_change": 2.0,
                "pe": 10.0,
                "pb": 1.5,
                "float_mv": 1000.0,
                "total_mv": 2000.0,
                "source_daily": "sw_daily",
                "source_basic": "sw_daily",
            }
        ]


class FailOnTradeDatePersistRepository(SequencedPersistRepository):
    def __init__(self, failing_trade_date: str) -> None:
        super().__init__()
        self.failing_trade_date = failing_trade_date

    async def upsert_daily_market(self, rows: list[object]) -> None:
        await super().upsert_daily_market(rows)
        trade_dates = {row.trade_date.strftime("%Y%m%d") for row in rows}
        if self.failing_trade_date in trade_dates:
            raise RuntimeError("persist failed")


class FailOnIndicatorPersistRepository(SequencedPersistRepository):
    def __init__(self, failing_trade_date: str) -> None:
        super().__init__()
        self.failing_trade_date = failing_trade_date

    async def upsert_daily_indicators(self, rows: list[object]) -> None:
        trade_dates = {row.trade_date.strftime("%Y%m%d") for row in rows}
        if self.failing_trade_date in trade_dates:
            raise RuntimeError("indicator persist failed")
        await super().upsert_daily_indicators(rows)



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
async def test_write_use_case_allows_cli_lookback_override(tmp_path: Path) -> None:
    provider = FlakyProvider()
    status_file = tmp_path / "last-write-status.txt"
    use_case = WriteMarketDataUseCase(
        settings=Settings(
            POSTGRES_DSN="postgresql://postgres:postgres@localhost:5432/stock_cache",
            TUSHARE_TOKEN="token",
            STATUS_FILE_PATH=status_file,
            DEFAULT_LOOKBACK_TRADING_DAYS=90,
        ),
        primary_provider=provider,
        market_repository=None,
        instrument_repository=None,
        job_run_repository=None,
        now_provider=lambda: datetime(2026, 3, 30, 12, tzinfo=UTC),
    )

    await use_case.run(mode="full", write_range=WriteDateRange(lookback_trading_days=2))

    assert provider.trade_date_requests == [("20260330", 2)]
    assert provider.trade_date_range_requests == []
    assert provider.batch_dates == ["20260330", "20260330", "20260327"]


@pytest.mark.asyncio
async def test_write_use_case_uses_absolute_trade_date_range(tmp_path: Path) -> None:
    provider = FlakyProvider()
    status_file = tmp_path / "last-write-status.txt"
    use_case = WriteMarketDataUseCase(
        settings=Settings(
            POSTGRES_DSN="postgresql://postgres:postgres@localhost:5432/stock_cache",
            TUSHARE_TOKEN="token",
            STATUS_FILE_PATH=status_file,
            DEFAULT_LOOKBACK_TRADING_DAYS=90,
        ),
        primary_provider=provider,
        market_repository=None,
        instrument_repository=None,
        job_run_repository=None,
        now_provider=lambda: datetime(2026, 3, 30, 12, tzinfo=UTC),
    )

    await use_case.run(
        mode="full",
        write_range=WriteDateRange(start_date="20260101", end_date="20260331"),
    )

    assert provider.trade_date_requests == []
    assert provider.trade_date_range_requests == [("20260101", "20260331")]
    assert provider.batch_dates == ["20260102", "20260102", "20260105", "20260331"]


@pytest.mark.asyncio
async def test_write_use_case_single_mode_uses_symbol_range_endpoints(tmp_path: Path) -> None:
    provider = SingleSymbolProvider()
    repository = RecordingMarketRepository()
    status_file = tmp_path / "last-write-status.txt"
    use_case = WriteMarketDataUseCase(
        settings=Settings(
            POSTGRES_DSN="postgresql://postgres:postgres@localhost:5432/stock_cache",
            TUSHARE_TOKEN="token",
            STATUS_FILE_PATH=status_file,
            DEFAULT_LOOKBACK_TRADING_DAYS=90,
        ),
        primary_provider=provider,
        market_repository=repository,
        instrument_repository=None,
        job_run_repository=None,
        now_provider=lambda: datetime(2026, 3, 30, 12, tzinfo=UTC),
    )

    summary = await use_case.run(mode="single", symbols=["000001.SZ"], write_range=WriteDateRange(lookback_trading_days=2))

    assert summary.status == "success"
    assert summary.success_symbols == ["000001.SZ"]
    assert provider.instrument_fetches == 0
    assert provider.trade_date_requests == [("20260330", 2)]
    assert provider.batch_dates == []
    assert provider.daily_requests == [("000001.SZ", "20260327", "20260330")]
    assert provider.daily_basic_requests == [("000001.SZ", "20260327", "20260330")]
    assert provider.moneyflow_requests == [("000001.SZ", "20260327", "20260330")]
    assert provider.indicator_requests == [("000001.SZ", "20260327", "20260330")]
    assert [row.ts_code for row in repository.market_rows] == ["000001.SZ", "000001.SZ"]
    assert [row.ts_code for row in repository.indicator_rows] == ["000001.SZ"]


@pytest.mark.asyncio
async def test_full_mode_persists_each_trade_date_immediately(tmp_path: Path) -> None:
    provider = StableTradeDateProvider()
    repository = SequencedPersistRepository()
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
    assert summary.success_symbols == ["000001.SZ"]
    assert summary.failed_symbols == {}
    assert repository.market_trade_dates_by_call == [
        ["20260330"],
        ["20260327"],
        ["20260326"],
        ["20260325"],
        ["20260324"],
    ]
    assert repository.indicator_trade_dates_by_call == [
        ["20260330"],
        ["20260327"],
        ["20260326"],
        ["20260325"],
        ["20260324"],
    ]


@pytest.mark.asyncio
async def test_full_mode_syncs_indexes_from_runtime_csv(tmp_path: Path) -> None:
    provider = IndexAwareProvider()
    repository = SequencedPersistRepository()
    status_file = tmp_path / "last-write-status.txt"
    index_list = tmp_path / "default-indexes.csv"
    index_list.write_text(
        "\n".join(
            [
                "ts_code,name,group_name,enabled",
                "000300.SH,沪深300,major,true",
                "801012.SI,农产品加工(申万),sw_secondary,true",
                "801250.SI,申万制造,theme,true",
            ]
        )
        + "\n",
        encoding="utf-8",
    )
    use_case = WriteMarketDataUseCase(
        settings=Settings(
            POSTGRES_DSN="postgresql://postgres:postgres@localhost:5432/stock_cache",
            TUSHARE_TOKEN="token",
            STATUS_FILE_PATH=status_file,
            INDEX_LIST_PATH=index_list,
        ),
        primary_provider=provider,
        market_repository=repository,
        instrument_repository=None,
        job_run_repository=None,
    )

    summary = await use_case.run(mode="full")

    assert summary.status == "success"
    assert summary.failed_symbols == {}
    assert provider.index_daily_requests == [("000300.SH", "20260324", "20260330")]
    assert provider.sw_daily_requests == [
        ("801012.SI", "20260324", "20260330"),
        ("801250.SI", "20260324", "20260330"),
    ]
    assert repository.index_codes_by_call == [["000300.SH", "000300.SH"], ["801012.SI"], ["801250.SI"]]


@pytest.mark.asyncio
async def test_run_indexes_only_syncs_indexes_without_loading_instruments(tmp_path: Path) -> None:
    provider = IndexAwareProvider()
    repository = SequencedPersistRepository()
    status_file = tmp_path / "last-write-status.txt"
    index_list = tmp_path / "default-indexes.csv"
    index_list.write_text(
        "\n".join(
            [
                "ts_code,name,group_name,enabled",
                "000300.SH,沪深300,major,true",
                "801012.SI,农产品加工(申万),sw_secondary,true",
            ]
        )
        + "\n",
        encoding="utf-8",
    )
    use_case = WriteMarketDataUseCase(
        settings=Settings(
            POSTGRES_DSN="postgresql://postgres:postgres@localhost:5432/stock_cache",
            TUSHARE_TOKEN="token",
            STATUS_FILE_PATH=status_file,
            INDEX_LIST_PATH=index_list,
        ),
        primary_provider=provider,
        market_repository=repository,
        instrument_repository=None,
        job_run_repository=None,
        now_provider=lambda: datetime(2026, 3, 30, 12, tzinfo=UTC),
    )

    summary = await use_case.run_indexes_only(write_range=WriteDateRange(lookback_trading_days=2))

    assert summary.status == "success"
    assert summary.total_symbols == 2
    assert summary.success_symbols == ["000300.SH", "801012.SI"]
    assert summary.failed_symbols == {}
    assert provider.instrument_fetches == 0
    assert provider.trade_date_requests == [("20260330", 2)]
    assert provider.index_daily_requests == [("000300.SH", "20260327", "20260330")]
    assert provider.sw_daily_requests == [("801012.SI", "20260327", "20260330")]


@pytest.mark.asyncio
async def test_full_mode_continues_after_trade_date_persist_failure(tmp_path: Path) -> None:
    provider = StableTradeDateProvider()
    repository = FailOnTradeDatePersistRepository("20260327")
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
    assert summary.success_symbols == []
    assert summary.failed_symbols == {"__trade_date__:20260327": "persist failed"}
    assert repository.market_trade_dates_by_call == [
        ["20260330"],
        ["20260327"],
        ["20260326"],
        ["20260325"],
        ["20260324"],
    ]
    assert repository.indicator_trade_dates_by_call == [
        ["20260330"],
        ["20260326"],
        ["20260325"],
        ["20260324"],
    ]


@pytest.mark.asyncio
async def test_full_mode_continues_after_indicator_persist_failure(tmp_path: Path) -> None:
    provider = StableTradeDateProvider()
    repository = FailOnIndicatorPersistRepository("20260327")
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
    assert summary.success_symbols == []
    assert summary.failed_symbols == {"__trade_date__:20260327": "indicator persist failed"}
    assert repository.market_trade_dates_by_call == [
        ["20260330"],
        ["20260327"],
        ["20260326"],
        ["20260325"],
        ["20260324"],
    ]
    assert repository.indicator_trade_dates_by_call == [
        ["20260330"],
        ["20260326"],
        ["20260325"],
        ["20260324"],
    ]


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
