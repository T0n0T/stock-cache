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


def test_akshare_adapter_fetch_daily_returns_empty_list_placeholder() -> None:
    adapter = AkshareAdapter()

    rows = adapter.fetch_daily("000001.SZ", "20260101", "20260330")

    assert rows == []
