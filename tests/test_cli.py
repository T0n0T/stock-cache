import json
from dataclasses import dataclass
from datetime import date

from typer.testing import CliRunner

from stock_cache.cli import app
from stock_cache.domain.models import DailyIndicatorRow, DailyMarketRow
from stock_cache.use_cases.read_screen import ReadScreeningResultsUseCase
from stock_cache.providers.akshare_adapter import AkshareAdapter
from stock_cache.providers.tushare_adapter import TushareAdapter
from stock_cache.use_cases.read_raw import ReadRawMarketDataUseCase


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

    def daily(self, *, ts_code: str, start_date: str, end_date: str) -> FakeFrame:
        self.calls.append(
            {"ts_code": ts_code, "start_date": start_date, "end_date": end_date}
        )
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

    monkeypatch.setattr("stock_cache.providers.tushare_adapter.ts.pro_api", fake_pro_api)

    adapter = TushareAdapter("demo-token")
    rows = adapter.fetch_daily("000001.SZ", "20260101", "20260330")

    assert captured["token"] == "demo-token"
    assert client.calls == [
        {"ts_code": "000001.SZ", "start_date": "20260101", "end_date": "20260330"}
    ]
    assert frame.orient == "records"
    assert rows == [{"ts_code": "000001.SZ", "trade_date": "20260330", "close": 12.4}]


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

    monkeypatch.setattr("stock_cache.providers.tushare_adapter.ts.pro_api", lambda token: client)

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


def test_tushare_adapter_fetch_recent_trade_dates_returns_open_days(monkeypatch) -> None:
    frame = FakeFrame(
        [
            {"cal_date": "20260331", "is_open": 1},
            {"cal_date": "20260330", "is_open": 1},
            {"cal_date": "20260329", "is_open": 0},
        ]
    )
    client = FakeTushareProClient(frame)

    monkeypatch.setattr("stock_cache.providers.tushare_adapter.ts.pro_api", lambda token: client)

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

    monkeypatch.setattr("stock_cache.providers.tushare_adapter.ts.pro_api", lambda token: client)

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
    assert rows == [{"trade_date": "20260331", "turnover_rate": 3.3, "total_mv": 456.7}]


def test_tushare_adapter_fetch_moneyflow_converts_dataframe_to_records(monkeypatch) -> None:
    frame = FakeFrame([{"trade_date": "20260331", "net_mf_amount": 9.8}])
    client = FakeTushareProClient(frame)

    monkeypatch.setattr("stock_cache.providers.tushare_adapter.ts.pro_api", lambda token: client)

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
    assert rows == [{"trade_date": "20260331", "net_mf_amount": 9.8}]


def test_tushare_adapter_fetch_indicators_converts_dataframe_to_records(monkeypatch) -> None:
    frame = FakeFrame([{"trade_date": "20260331", "macd": 0.11, "kdj_j": 81.0}])
    client = FakeTushareProClient(frame)

    monkeypatch.setattr("stock_cache.providers.tushare_adapter.ts.pro_api", lambda token: client)

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


def test_akshare_adapter_fetch_daily_returns_empty_list_placeholder() -> None:
    adapter = AkshareAdapter()

    rows = adapter.fetch_daily("000001.SZ", "20260101", "20260330")

    assert rows == []
