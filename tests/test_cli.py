import json
from datetime import date

from typer.testing import CliRunner

from stock_cache.cli import app
from stock_cache.domain.models import DailyIndicatorRow, DailyMarketRow
from stock_cache.use_cases.read_raw import ReadRawMarketDataUseCase


runner = CliRunner()


def test_cli_help_lists_write_and_read_commands() -> None:
    result = runner.invoke(app, ["--help"])

    assert result.exit_code == 0
    assert "write" in result.stdout
    assert "read" in result.stdout


class FakeMarketRepository:
    async def fetch_raw(self, ts_code: str, start_date: str, end_date: str) -> dict[str, list[object]]:
        return {
            "market": [DailyMarketRow(ts_code=ts_code, trade_date=date(2026, 3, 30), close=12.4)],
            "indicators": [DailyIndicatorRow(ts_code=ts_code, trade_date=date(2026, 3, 30), macd=0.1)],
        }


def test_cli_read_raw_prints_json_payload() -> None:
    result = runner.invoke(
        app,
        ["read", "raw", "000001.SZ", "2026-01-01", "2026-03-30"],
        obj={"read_raw_use_case": ReadRawMarketDataUseCase(FakeMarketRepository())},
    )

    assert result.exit_code == 0
    payload = json.loads(result.stdout)
    assert payload["query"]["ts_code"] == "000001.SZ"
    assert payload["meta"]["row_count_market"] == 1
    assert payload["meta"]["row_count_indicators"] == 1
