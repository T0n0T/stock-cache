import json
import time
from dataclasses import dataclass
from datetime import date
from requests import ConnectionError as RequestsConnectionError

import pytest
from typer.testing import CliRunner

import cli as cli_module
from cli import app
from domain.models import DailyIndicatorRow, DailyMarketRow, Instrument
from providers.tushare_adapter import TushareAdapter
from use_cases.read_raw import ReadRawMarketDataUseCase
from use_cases.read_screen import ReadScreeningResultsUseCase


runner = CliRunner()


def test_cli_help_lists_write_and_read_commands() -> None:
    result = runner.invoke(app, ["--help"])

    assert result.exit_code == 0
    assert "write" in result.stdout
    assert "read" in result.stdout


def test_read_help_lists_raw_and_screen_subcommands() -> None:
    result = runner.invoke(app, ["read", "--help"])

    assert result.exit_code == 0
    assert "raw" in result.stdout
    assert "screen" in result.stdout


class FakeMarketRepository:
    async def fetch_raw(self, ts_code: str, start_date: str, end_date: str) -> dict[str, list[object]]:
        return {
            "market": [DailyMarketRow(ts_code=ts_code, trade_date=date(2026, 3, 30), close=12.4)],
            "indicators": [DailyIndicatorRow(ts_code=ts_code, trade_date=date(2026, 3, 30), macd=0.1)],
        }


def test_cli_read_raw_prints_json_payload() -> None:
    result = runner.invoke(
        app,
        ["read", "raw", "--ts-code", "000001.SZ", "--start-date", "2026-01-01", "--end-date", "2026-03-30"],
        obj={"read_raw_use_case": ReadRawMarketDataUseCase(FakeMarketRepository())},
    )

    assert result.exit_code == 0
    payload = json.loads(result.stdout)
    assert payload["query"]["ts_code"] == "000001.SZ"
    assert payload["meta"]["row_count_market"] == 1
    assert payload["meta"]["row_count_indicators"] == 1


class FakeScreenRepository:
    async def screen(self, trade_date: str, filters: dict[str, object]) -> list[dict[str, object]]:
        return [
            {
                "ts_code": "300001.SZ",
                "trade_date": trade_date,
                "pct_chg": filters["pct_chg_gte"],
                "turnover_rate": filters["turnover_rate_gte"],
                "macd": filters["macd_gte"],
            }
        ]


def test_cli_read_screen_prints_json_payload() -> None:
    result = runner.invoke(
        app,
        [
            "read",
            "screen",
            "--trade-date",
            "2026-03-30",
            "--pct-chg-gte",
            "5",
            "--turnover-rate-gte",
            "3",
            "--macd-gte",
            "0",
        ],
        obj={
            "read_screen_use_case": ReadScreeningResultsUseCase(
                FakeScreenRepository(),
                indicator_service=None,
            )
        },
    )

    assert result.exit_code == 0
    payload = json.loads(result.stdout)
    assert payload["query"]["trade_date"] == "2026-03-30"
    assert payload["query"]["filters"] == {
        "pct_chg_gte": 5.0,
        "turnover_rate_gte": 3.0,
        "macd_gte": 0.0,
    }
    assert payload["meta"]["matched"] == 1
    assert payload["data"][0]["ts_code"] == "300001.SZ"


class FakePool:
    def __init__(self) -> None:
        self.closed = False

    async def close(self) -> None:
        self.closed = True


def test_run_read_raw_closes_pool_when_building_live_repository(monkeypatch, sample_dsn: str) -> None:
    pool = FakePool()

    class FakeReadRawUseCase:
        def __init__(self, market_repository: object) -> None:
            self.market_repository = market_repository

        async def run(self, ts_code: str, start_date: str, end_date: str) -> dict[str, object]:
            _ = self.market_repository
            return {"query": {"ts_code": ts_code, "start_date": start_date, "end_date": end_date}}

    async def fake_create_pool(dsn: str) -> FakePool:
        assert dsn == sample_dsn
        return pool

    monkeypatch.setenv("POSTGRES_DSN", sample_dsn)
    monkeypatch.setenv("TUSHARE_TOKEN", "token")
    monkeypatch.setattr("cli.create_pool", fake_create_pool)
    monkeypatch.setattr("cli.ReadRawMarketDataUseCase", FakeReadRawUseCase)

    payload = cli_module.asyncio.run(
        cli_module._run_read_raw(
            ts_code="000001.SZ",
            start_date="2026-03-01",
            end_date="2026-03-31",
            injected_use_case=None,
        )
    )

    assert payload["query"]["ts_code"] == "000001.SZ"
    assert pool.closed is True


def test_run_read_screen_closes_pool_when_building_live_repository(monkeypatch, sample_dsn: str) -> None:
    pool = FakePool()

    class FakeReadScreenUseCase:
        def __init__(self, market_repository: object, indicator_service: object | None) -> None:
            self.market_repository = market_repository
            self.indicator_service = indicator_service

        async def run(self, trade_date: str, filters: dict[str, object]) -> dict[str, object]:
            _ = (self.market_repository, self.indicator_service)
            return {"query": {"trade_date": trade_date, "filters": filters}}

    async def fake_create_pool(dsn: str) -> FakePool:
        assert dsn == sample_dsn
        return pool

    monkeypatch.setenv("POSTGRES_DSN", sample_dsn)
    monkeypatch.setenv("TUSHARE_TOKEN", "token")
    monkeypatch.setattr("cli.create_pool", fake_create_pool)
    monkeypatch.setattr("cli.ReadScreeningResultsUseCase", FakeReadScreenUseCase)

    payload = cli_module.asyncio.run(
        cli_module._run_read_screen(
            trade_date="2026-03-31",
            filters={"pct_chg_gte": 5.0},
            injected_use_case=None,
        )
    )

    assert payload["query"]["trade_date"] == "2026-03-31"
    assert pool.closed is True


@dataclass
class FakeJobRunSummary:
    job_id: str
    status: str
    started_at: str
    finished_at: str
    total_symbols: int
    success_symbols: list[str]
    failed_symbols: dict[str, str]


class FakeWriteUseCase:
    def __init__(self) -> None:
        self.calls: list[tuple[str, list[str] | None]] = []

    async def run(self, mode: str, symbols: list[str] | None = None) -> FakeJobRunSummary:
        self.calls.append((mode, symbols))
        return FakeJobRunSummary(
            job_id="20260331T120000Z",
            status="success",
            started_at="2026-03-31T12:00:00+00:00",
            finished_at="2026-03-31T12:00:01+00:00",
            total_symbols=1,
            success_symbols=["000001.SZ"],
            failed_symbols={},
        )


def test_cli_write_runs_use_case_with_mode_option() -> None:
    use_case = FakeWriteUseCase()

    result = runner.invoke(
        app,
        ["write", "--mode", "full"],
        obj={"write_use_case": use_case},
    )

    assert result.exit_code == 0
    assert use_case.calls == [("full", None)]
    payload = json.loads(result.stdout)
    assert payload["status"] == "success"
    assert payload["success_symbols"] == ["000001.SZ"]


def test_cli_write_does_not_instantiate_akshare_adapter(monkeypatch, sample_dsn: str, tmp_path) -> None:
    monkeypatch.setenv("POSTGRES_DSN", sample_dsn)
    monkeypatch.setenv("TUSHARE_TOKEN", "token")
    monkeypatch.setenv("STATUS_FILE_PATH", str(tmp_path / "status.txt"))
    monkeypatch.setenv("DEFAULT_LOOKBACK_TRADING_DAYS", "1")

    def fail_akshare_init() -> None:
        raise AssertionError("AkshareAdapter should not be instantiated")

    def fake_fetch_instruments(self: object) -> list[Instrument]:
        _ = self
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

    def fake_fetch_recent_trade_dates(self: object, end_date: str, limit: int) -> list[str]:
        _ = (self, end_date, limit)
        return ["20260331"]

    async def fake_fetch_daily_by_trade_date(self: object, trade_date: str) -> list[dict[str, object]]:
        _ = (self, trade_date)
        return [{"ts_code": "000001.SZ", "trade_date": "20260331", "close": 11.6, "pct_chg": 2.2}]

    async def fake_fetch_daily_basic_by_trade_date(self: object, trade_date: str) -> list[dict[str, object]]:
        _ = (self, trade_date)
        return [{"ts_code": "000001.SZ", "trade_date": "20260331", "turnover_rate": 3.3, "total_mv": 456.7}]

    async def fake_fetch_moneyflow_by_trade_date(self: object, trade_date: str) -> list[dict[str, object]]:
        _ = (self, trade_date)
        return [{"ts_code": "000001.SZ", "trade_date": "20260331", "net_mf_amount": 9.8}]

    async def fake_fetch_adj_factor_by_trade_date(self: object, trade_date: str) -> list[dict[str, object]]:
        _ = (self, trade_date)
        return []

    async def fake_fetch_stk_limit_by_trade_date(self: object, trade_date: str) -> list[dict[str, object]]:
        _ = (self, trade_date)
        return []

    async def fake_fetch_suspend_d_by_trade_date(self: object, trade_date: str) -> list[dict[str, object]]:
        _ = (self, trade_date)
        return []

    async def fake_fetch_indicators_by_trade_date(self: object, trade_date: str) -> list[dict[str, object]]:
        _ = (self, trade_date)
        return [{"ts_code": "000001.SZ", "trade_date": "20260331", "macd": 0.11, "kdj_j": 81.0}]

    monkeypatch.setattr(cli_module, "AkshareAdapter", fail_akshare_init, raising=False)
    monkeypatch.setattr("providers.tushare_adapter.ts.pro_api", lambda token: object())
    monkeypatch.setattr("providers.tushare_adapter.TushareAdapter.fetch_instruments", fake_fetch_instruments)
    monkeypatch.setattr(
        "providers.tushare_adapter.TushareAdapter.fetch_recent_trade_dates",
        fake_fetch_recent_trade_dates,
    )
    monkeypatch.setattr(
        "providers.tushare_adapter.TushareAdapter.fetch_daily_by_trade_date",
        fake_fetch_daily_by_trade_date,
    )
    monkeypatch.setattr(
        "providers.tushare_adapter.TushareAdapter.fetch_daily_basic_by_trade_date",
        fake_fetch_daily_basic_by_trade_date,
    )
    monkeypatch.setattr(
        "providers.tushare_adapter.TushareAdapter.fetch_moneyflow_by_trade_date",
        fake_fetch_moneyflow_by_trade_date,
    )
    monkeypatch.setattr(
        "providers.tushare_adapter.TushareAdapter.fetch_adj_factor_by_trade_date",
        fake_fetch_adj_factor_by_trade_date,
    )
    monkeypatch.setattr(
        "providers.tushare_adapter.TushareAdapter.fetch_stk_limit_by_trade_date",
        fake_fetch_stk_limit_by_trade_date,
    )
    monkeypatch.setattr(
        "providers.tushare_adapter.TushareAdapter.fetch_suspend_d_by_trade_date",
        fake_fetch_suspend_d_by_trade_date,
    )
    monkeypatch.setattr(
        "providers.tushare_adapter.TushareAdapter.fetch_indicators_by_trade_date",
        fake_fetch_indicators_by_trade_date,
    )

    result = runner.invoke(app, ["write", "--mode", "full"])

    assert result.exit_code == 0
    payload = json.loads(result.stdout)
    assert payload["status"] == "success"


def test_cli_init_db_prints_json_payload(monkeypatch) -> None:
    async def fake_run_init_db(injected_result: object | None = None) -> dict[str, object]:
        _ = injected_result
        return {
            "status": "ok",
            "created_tables": ["daily_market"],
            "already_present": ["instruments", "daily_indicators", "job_runs"],
            "missing": [],
        }

    monkeypatch.setattr("cli._run_init_db", fake_run_init_db, raising=False)

    result = runner.invoke(app, ["init-db"])

    assert result.exit_code == 0
    payload = json.loads(result.stdout)
    assert payload == {
        "status": "ok",
        "created_tables": ["daily_market"],
        "already_present": ["instruments", "daily_indicators", "job_runs"],
        "missing": [],
    }


class FakeFrame:
    def __init__(self, records: list[dict[str, object]]) -> None:
        self.records = records
        self.orient: str | None = None

    def to_dict(self, orient: str) -> list[dict[str, object]]:
        self.orient = orient
        return self.records


class FakeTushareProClient:
    def __init__(self, frame: FakeFrame) -> None:
        self.frame = frame
        self.calls: list[dict[str, str]] = []

    def daily(self, **kwargs: str) -> FakeFrame:
        self.calls.append({"endpoint": "daily", **kwargs})
        return self.frame

    def stock_basic(self, **kwargs: str) -> FakeFrame:
        self.calls.append({"endpoint": "stock_basic", **kwargs})
        return self.frame

    def trade_cal(self, **kwargs: str) -> FakeFrame:
        self.calls.append({"endpoint": "trade_cal", **kwargs})
        return self.frame

    def daily_basic(self, **kwargs: str) -> FakeFrame:
        self.calls.append({"endpoint": "daily_basic", **kwargs})
        return self.frame

    def moneyflow(self, **kwargs: str) -> FakeFrame:
        self.calls.append({"endpoint": "moneyflow", **kwargs})
        return self.frame

    def stk_factor(self, **kwargs: str) -> FakeFrame:
        self.calls.append({"endpoint": "stk_factor", **kwargs})
        return self.frame


def test_tushare_adapter_fetch_daily_converts_dataframe_to_records(monkeypatch) -> None:
    captured: dict[str, object] = {}
    frame = FakeFrame([{"ts_code": "000001.SZ", "trade_date": "20260330", "close": 12.4}])
    client = FakeTushareProClient(frame)

    def fake_pro_api(token: str) -> FakeTushareProClient:
        captured["token"] = token
        return client

    monkeypatch.setattr("providers.tushare_adapter.ts.pro_api", fake_pro_api)

    adapter = TushareAdapter("demo-token")
    rows = adapter.fetch_daily("000001.SZ", "20260101", "20260330")

    assert captured["token"] == "demo-token"
    assert client.calls == [
        {"endpoint": "daily", "ts_code": "000001.SZ", "start_date": "20260101", "end_date": "20260330"}
    ]
    assert frame.orient == "records"
    assert rows == [{"ts_code": "000001.SZ", "trade_date": "20260330", "close": 12.4, "source_daily": "daily"}]


def test_tushare_adapter_fetch_instruments_maps_stock_basic_rows(monkeypatch) -> None:
    frame = FakeFrame(
        [
            {
                "ts_code": "000001.SZ",
                "symbol": "000001",
                "name": "Ping An Bank",
                "exchange": "SZSE",
                "list_status": "L",
            }
        ]
    )
    client = FakeTushareProClient(frame)

    monkeypatch.setattr("providers.tushare_adapter.ts.pro_api", lambda token: client)

    adapter = TushareAdapter("demo-token")
    instruments = list(adapter.fetch_instruments())

    assert client.calls == [{"endpoint": "stock_basic", "list_status": "L"}]
    assert len(instruments) == 1
    assert instruments[0].ts_code == "000001.SZ"
    assert instruments[0].symbol == "000001"
    assert instruments[0].name == "Ping An Bank"
    assert instruments[0].exchange == "SZSE"
    assert instruments[0].list_status == "L"
    assert instruments[0].is_st is False


def test_tushare_adapter_fetch_instruments_infers_exchange_from_ts_code_when_missing(monkeypatch) -> None:
    frame = FakeFrame(
        [
            {
                "ts_code": "600000.SH",
                "symbol": "600000",
                "name": "浦发银行",
                "list_status": "L",
            }
        ]
    )
    client = FakeTushareProClient(frame)

    monkeypatch.setattr("providers.tushare_adapter.ts.pro_api", lambda token: client)

    adapter = TushareAdapter("demo-token")
    instruments = list(adapter.fetch_instruments())

    assert len(instruments) == 1
    assert instruments[0].exchange == "SSE"
    assert instruments[0].list_status == "L"


def test_tushare_adapter_fetch_recent_trade_dates_returns_open_days(monkeypatch) -> None:
    frame = FakeFrame(
        [
            {"cal_date": "20260331", "is_open": 1},
            {"cal_date": "20260330", "is_open": 1},
            {"cal_date": "20260329", "is_open": 0},
        ]
    )
    client = FakeTushareProClient(frame)

    monkeypatch.setattr("providers.tushare_adapter.ts.pro_api", lambda token: client)

    adapter = TushareAdapter("demo-token")
    dates = list(adapter.fetch_recent_trade_dates("20260331", 2))

    assert client.calls == [
        {
            "endpoint": "trade_cal",
            "exchange": "SSE",
            "start_date": "20260324",
            "end_date": "20260331",
        }
    ]
    assert dates == ["20260331", "20260330"]


def test_tushare_adapter_fetch_daily_basic_converts_dataframe_to_records(monkeypatch) -> None:
    frame = FakeFrame([{"trade_date": "20260331", "turnover_rate": 3.3, "total_mv": 456.7}])
    client = FakeTushareProClient(frame)

    monkeypatch.setattr("providers.tushare_adapter.ts.pro_api", lambda token: client)

    adapter = TushareAdapter("demo-token")
    rows = adapter.fetch_daily_basic("000001.SZ", "20260324", "20260331")

    assert client.calls == [
        {
            "endpoint": "daily_basic",
            "ts_code": "000001.SZ",
            "start_date": "20260324",
            "end_date": "20260331",
        }
    ]
    assert rows == [
        {"trade_date": "20260331", "turnover_rate": 3.3, "total_mv": 456.7, "source_daily_basic": "daily_basic"}
    ]


def test_tushare_adapter_fetch_moneyflow_converts_dataframe_to_records(monkeypatch) -> None:
    frame = FakeFrame([{"trade_date": "20260331", "net_mf_amount": 9.8}])
    client = FakeTushareProClient(frame)

    monkeypatch.setattr("providers.tushare_adapter.ts.pro_api", lambda token: client)

    adapter = TushareAdapter("demo-token")
    rows = adapter.fetch_moneyflow("000001.SZ", "20260324", "20260331")

    assert client.calls == [
        {
            "endpoint": "moneyflow",
            "ts_code": "000001.SZ",
            "start_date": "20260324",
            "end_date": "20260331",
        }
    ]
    assert rows == [{"trade_date": "20260331", "net_mf_amount": 9.8, "source_moneyflow": "moneyflow"}]


def test_tushare_adapter_fetch_indicators_converts_dataframe_to_records(monkeypatch) -> None:
    frame = FakeFrame([{"trade_date": "20260331", "macd": 0.11, "kdj_j": 81.0}])
    client = FakeTushareProClient(frame)

    monkeypatch.setattr("providers.tushare_adapter.ts.pro_api", lambda token: client)

    adapter = TushareAdapter("demo-token")
    rows = adapter.fetch_indicators("000001.SZ", "20260324", "20260331")

    assert client.calls == [
        {
            "endpoint": "stk_factor",
            "ts_code": "000001.SZ",
            "start_date": "20260324",
            "end_date": "20260331",
        }
    ]
    assert rows == [{"trade_date": "20260331", "macd": 0.11, "kdj_j": 81.0}]


@pytest.mark.asyncio
async def test_tushare_adapter_fetch_daily_by_trade_date_converts_dataframe_to_records(monkeypatch) -> None:
    frame = FakeFrame([{"ts_code": "000001.SZ", "trade_date": "20260331", "close": 12.4}])
    client = FakeTushareProClient(frame)

    monkeypatch.setattr("providers.tushare_adapter.ts.pro_api", lambda token: client)

    adapter = TushareAdapter("demo-token")
    rows = await adapter.fetch_daily_by_trade_date("20260331")

    assert client.calls == [
        {"endpoint": "daily", "trade_date": "20260331"}
    ]
    assert rows == [{"ts_code": "000001.SZ", "trade_date": "20260331", "close": 12.4, "source_daily": "daily"}]


@pytest.mark.asyncio
async def test_tushare_adapter_fetch_indicators_by_trade_date_falls_back_to_stk_factor(monkeypatch) -> None:
    fallback_frame = FakeFrame([{"ts_code": "000001.SZ", "trade_date": "20260331", "macd": 0.11}])

    class FactorFallbackClient:
        def __init__(self) -> None:
            self.calls: list[dict[str, str]] = []

        def stk_factor_pro(self, **kwargs: str) -> FakeFrame:
            self.calls.append({"endpoint": "stk_factor_pro", **kwargs})
            raise Exception("抱歉，您没有访问该接口的权限")

        def stk_factor(self, **kwargs: str) -> FakeFrame:
            self.calls.append({"endpoint": "stk_factor", **kwargs})
            return fallback_frame

    client = FactorFallbackClient()
    monkeypatch.setattr("providers.tushare_adapter.ts.pro_api", lambda token: client)

    adapter = TushareAdapter("demo-token")
    rows = await adapter.fetch_indicators_by_trade_date("20260331")

    assert client.calls == [
        {"endpoint": "stk_factor_pro", "trade_date": "20260331"},
        {"endpoint": "stk_factor", "trade_date": "20260331"},
    ]
    assert rows == [
        {
            "ts_code": "000001.SZ",
            "trade_date": "20260331",
            "macd": 0.11,
            "source_interface": "stk_factor",
        }
    ]


def test_tushare_adapter_wraps_network_errors_as_retryable_provider_error(monkeypatch) -> None:
    class FailingClient:
        def daily(self, **kwargs: str) -> FakeFrame:
            raise RequestsConnectionError("dns failed")

    monkeypatch.setattr("providers.tushare_adapter.ts.pro_api", lambda token: FailingClient())

    adapter = TushareAdapter("demo-token")

    from domain.errors import RetryableProviderError

    with pytest.raises(RetryableProviderError, match="dns failed"):
        adapter.fetch_daily("000001.SZ", "20260324", "20260331")


def test_tushare_adapter_wraps_slow_calls_as_retryable_provider_error(monkeypatch) -> None:
    class SlowClient:
        def daily(self, **kwargs: str) -> FakeFrame:
            time.sleep(0.05)
            return FakeFrame([])

    monkeypatch.setattr("providers.tushare_adapter.ts.pro_api", lambda token: SlowClient())

    adapter = TushareAdapter("demo-token", timeout_seconds=0.01)

    from domain.errors import RetryableProviderError

    started = time.perf_counter()
    with pytest.raises(RetryableProviderError, match="timed out"):
        adapter.fetch_daily("000001.SZ", "20260324", "20260331")
    assert time.perf_counter() - started < 0.03
