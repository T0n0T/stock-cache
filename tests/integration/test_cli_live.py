import json
from datetime import date
import os
from pathlib import Path
import subprocess
import sys

from typer.testing import CliRunner

from cli import app
from db.pool import create_pool
from domain.models import DailyIndicatorRow, DailyMarketRow, Instrument
from repositories.market_data import MarketDataRepository


runner = CliRunner()


async def _seed_live_rows(sample_dsn: str, ts_code: str) -> None:
    pool = await create_pool(sample_dsn)
    repository = MarketDataRepository(pool)
    try:
        await repository.upsert_daily_market(
            [
                DailyMarketRow(
                    ts_code=ts_code,
                    trade_date=date(2026, 3, 31),
                    close=12.4,
                    pct_chg=6.2,
                    turnover_rate=4.8,
                    total_mv=28000000000.0,
                    net_mf_amount=12.0,
                    source_provider="integration-test",
                )
            ]
        )
        await repository.upsert_daily_indicators(
            [
                DailyIndicatorRow(
                    ts_code=ts_code,
                    trade_date=date(2026, 3, 31),
                    macd=0.13,
                    kdj_j=91.4,
                    source_provider="integration-test",
                    source_interface="integration",
                )
            ]
        )
    finally:
        await pool.close()


async def _cleanup_live_rows(sample_dsn: str, ts_code: str) -> None:
    pool = await create_pool(sample_dsn)
    try:
        async with pool.acquire() as connection:
            await connection.execute("delete from daily_indicators where ts_code = $1", ts_code)
            await connection.execute("delete from daily_market where ts_code = $1", ts_code)
    finally:
        await pool.close()


def test_cli_read_raw_uses_live_postgres_when_context_is_not_injected(
    monkeypatch, sample_dsn: str
) -> None:
    ts_code = "LIVERAW1.SZ"
    monkeypatch.setenv("POSTGRES_DSN", sample_dsn)
    monkeypatch.setenv("TUSHARE_TOKEN", "token")

    import asyncio

    asyncio.run(_seed_live_rows(sample_dsn, ts_code))
    try:
        result = runner.invoke(
            app,
            [
                "read",
                "raw",
                "--ts-code",
                ts_code,
                "--start-date",
                "2026-03-01",
                "--end-date",
                "2026-03-31",
            ],
        )
    finally:
        asyncio.run(_cleanup_live_rows(sample_dsn, ts_code))

    assert result.exit_code == 0
    payload = json.loads(result.stdout)
    assert payload["query"]["ts_code"] == ts_code
    assert payload["meta"]["row_count_market"] == 1
    assert payload["meta"]["row_count_indicators"] == 1
    assert payload["data"]["market"][0]["close"] == 12.4


def test_cli_read_screen_uses_live_postgres_when_context_is_not_injected(
    monkeypatch, sample_dsn: str
) -> None:
    ts_code = "LIVESCR1.SZ"
    monkeypatch.setenv("POSTGRES_DSN", sample_dsn)
    monkeypatch.setenv("TUSHARE_TOKEN", "token")

    import asyncio

    asyncio.run(_seed_live_rows(sample_dsn, ts_code))
    try:
        result = runner.invoke(
            app,
            [
                "read",
                "screen",
                "--trade-date",
                "2026-03-31",
                "--pct-chg-gte",
                "5",
                "--turnover-rate-gte",
                "3",
                "--macd-gte",
                "0",
            ],
        )
    finally:
        asyncio.run(_cleanup_live_rows(sample_dsn, ts_code))

    assert result.exit_code == 0
    payload = json.loads(result.stdout)
    assert payload["meta"]["matched"] >= 1
    assert any(row["ts_code"] == ts_code for row in payload["data"])


def test_cli_read_raw_exits_and_prints_json_in_real_subprocess(monkeypatch, sample_dsn: str) -> None:
    ts_code = "LIVERAW2.SZ"
    monkeypatch.setenv("POSTGRES_DSN", sample_dsn)
    monkeypatch.setenv("TUSHARE_TOKEN", "token")

    import asyncio

    asyncio.run(_seed_live_rows(sample_dsn, ts_code))
    try:
        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "cli",
                "read",
                "raw",
                "--ts-code",
                ts_code,
                "--start-date",
                "2026-03-01",
                "--end-date",
                "2026-03-31",
            ],
            capture_output=True,
            check=False,
            env={
                "PATH": os.environ["PATH"],
                "POSTGRES_DSN": sample_dsn,
                "PYTHONPATH": "src",
                "TUSHARE_TOKEN": "token",
            },
            text=True,
            timeout=5,
        )
    finally:
        asyncio.run(_cleanup_live_rows(sample_dsn, ts_code))

    assert result.returncode == 0
    payload = json.loads(result.stdout)
    assert payload["query"]["ts_code"] == ts_code
    assert payload["meta"]["row_count_market"] == 1
    assert payload["data"]["market"][0]["trade_date"] == "2026-03-31"


def test_cli_read_screen_exits_and_prints_json_in_real_subprocess(monkeypatch, sample_dsn: str) -> None:
    ts_code = "LIVESCR2.SZ"
    monkeypatch.setenv("POSTGRES_DSN", sample_dsn)
    monkeypatch.setenv("TUSHARE_TOKEN", "token")

    import asyncio

    asyncio.run(_seed_live_rows(sample_dsn, ts_code))
    try:
        result = subprocess.run(
            [
                sys.executable,
                "-m",
                "cli",
                "read",
                "screen",
                "--trade-date",
                "2026-03-31",
                "--pct-chg-gte",
                "5",
                "--turnover-rate-gte",
                "3",
                "--macd-gte",
                "0",
            ],
            capture_output=True,
            check=False,
            env={
                "PATH": os.environ["PATH"],
                "POSTGRES_DSN": sample_dsn,
                "PYTHONPATH": "src",
                "TUSHARE_TOKEN": "token",
            },
            text=True,
            timeout=5,
        )
    finally:
        asyncio.run(_cleanup_live_rows(sample_dsn, ts_code))

    assert result.returncode == 0
    payload = json.loads(result.stdout)
    assert payload["meta"]["matched"] >= 1
    assert any(row["ts_code"] == ts_code for row in payload["data"])


def test_cli_write_uses_live_postgres_when_context_is_not_injected(
    monkeypatch, sample_dsn: str, tmp_path: Path
) -> None:
    ts_code = "LIVECLIW1.SZ"
    status_file = tmp_path / "last-write-status.txt"
    monkeypatch.setenv("POSTGRES_DSN", sample_dsn)
    monkeypatch.setenv("TUSHARE_TOKEN", "token")
    monkeypatch.setenv("STATUS_FILE_PATH", str(status_file))
    monkeypatch.setenv("DEFAULT_LOOKBACK_TRADING_DAYS", "2")
    monkeypatch.setattr(
        "cli.AkshareAdapter",
        lambda: (_ for _ in ()).throw(AssertionError("AkshareAdapter should not be instantiated")),
        raising=False,
    )

    def fake_fetch_instruments(self: object) -> list[Instrument]:
        _ = self
        return [
            Instrument(
                ts_code=ts_code,
                symbol="LIVECLIW1",
                name="Live CLI Write",
                exchange="SZ",
                list_status="L",
                is_st=False,
            )
        ]

    def fake_fetch_recent_trade_dates(self: object, end_date: str, limit: int) -> list[str]:
        _ = (self, end_date)
        return ["20260331", "20260330"][:limit]

    async def fake_fetch_daily_by_trade_date(self: object, trade_date: str) -> list[dict[str, object]]:
        _ = self
        if trade_date != "20260331":
            return []
        return [{"ts_code": ts_code, "trade_date": "20260331", "close": 11.6, "pct_chg": 2.2}]

    async def fake_fetch_daily_basic_by_trade_date(self: object, trade_date: str) -> list[dict[str, object]]:
        _ = self
        if trade_date != "20260331":
            return []
        return [{"ts_code": ts_code, "trade_date": "20260331", "turnover_rate": 3.3, "total_mv": 456.7}]

    async def fake_fetch_moneyflow_by_trade_date(self: object, trade_date: str) -> list[dict[str, object]]:
        _ = self
        if trade_date != "20260331":
            return []
        return [{"ts_code": ts_code, "trade_date": "20260331", "net_mf_amount": 9.8}]

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
        _ = self
        if trade_date != "20260331":
            return []
        return [{"ts_code": ts_code, "trade_date": "20260331", "macd": 0.11, "kdj_j": 81.0}]

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

    import asyncio

    async def verify() -> tuple[object, object]:
        pool = await create_pool(sample_dsn)
        try:
            async with pool.acquire() as connection:
                market = await connection.fetchrow(
                    "select close, turnover_rate, total_mv, net_mf_amount from daily_market where ts_code = $1 and trade_date = date '2026-03-31'",
                    ts_code,
                )
                indicator = await connection.fetchrow(
                    "select macd, kdj_j from daily_indicators where ts_code = $1 and trade_date = date '2026-03-31'",
                    ts_code,
                )
                instrument = await connection.fetchrow(
                    "select symbol, name, exchange, list_status, is_st from instruments where ts_code = $1",
                    ts_code,
                )
                job_run = await connection.fetchrow(
                    """
                    select status, total_symbols, success_symbols, failed_symbols, status_file_path
                    from job_runs
                    order by started_at desc
                    limit 1
                    """
                )
                return market, indicator, instrument, job_run
        finally:
            async with pool.acquire() as connection:
                await connection.execute("delete from job_runs where status_file_path = $1", str(status_file))
                await connection.execute("delete from instruments where ts_code = $1", ts_code)
                await connection.execute("delete from daily_indicators where ts_code = $1", ts_code)
                await connection.execute("delete from daily_market where ts_code = $1", ts_code)
            await pool.close()

    market, indicator, instrument, job_run = asyncio.run(verify())

    assert result.exit_code == 0
    payload = json.loads(result.stdout)
    assert payload["success_symbols"] == [ts_code]
    assert status_file.exists()
    assert market is not None
    assert market["close"] == 11.6
    assert market["turnover_rate"] == 3.3
    assert market["total_mv"] == 456.7
    assert market["net_mf_amount"] == 9.8
    assert indicator is not None
    assert indicator["macd"] == 0.11
    assert indicator["kdj_j"] == 81.0
    assert instrument is not None
    assert instrument["symbol"] == "LIVECLIW1"
    assert instrument["name"] == "Live CLI Write"
    assert instrument["exchange"] == "SZ"
    assert instrument["list_status"] == "L"
    assert instrument["is_st"] is False
    assert job_run is not None
    assert job_run["status"] == "success"
    assert job_run["total_symbols"] == 1
    assert job_run["success_symbols"] == 1
    assert job_run["failed_symbols"] == 0
    assert job_run["status_file_path"] == str(status_file)
