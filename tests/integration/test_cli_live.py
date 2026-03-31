import json
from datetime import date
from pathlib import Path

from typer.testing import CliRunner

from stock_cache.cli import app
from stock_cache.db.pool import create_pool
from stock_cache.domain.models import DailyIndicatorRow, DailyMarketRow, Instrument
from stock_cache.repositories.market_data import MarketDataRepository


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


def test_cli_write_uses_live_postgres_when_context_is_not_injected(
    monkeypatch, sample_dsn: str, tmp_path: Path
) -> None:
    ts_code = "LIVECLIW1.SZ"
    status_file = tmp_path / "last-write-status.txt"
    monkeypatch.setenv("POSTGRES_DSN", sample_dsn)
    monkeypatch.setenv("TUSHARE_TOKEN", "token")
    monkeypatch.setenv("STATUS_FILE_PATH", str(status_file))
    monkeypatch.setenv("DEFAULT_LOOKBACK_TRADING_DAYS", "2")

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

    def fake_fetch_daily(self: object, ts_code_arg: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        _ = (self, start_date, end_date)
        return [{"trade_date": "20260331", "close": 11.6, "pct_chg": 2.2}]

    def fake_fetch_daily_basic(self: object, ts_code_arg: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        _ = (self, ts_code_arg, start_date, end_date)
        return [{"trade_date": "20260331", "turnover_rate": 3.3, "total_mv": 456.7}]

    def fake_fetch_moneyflow(self: object, ts_code_arg: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        _ = (self, ts_code_arg, start_date, end_date)
        return [{"trade_date": "20260331", "net_mf_amount": 9.8}]

    def fake_fetch_indicators(self: object, ts_code_arg: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        _ = (self, ts_code_arg, start_date, end_date)
        return [{"trade_date": "20260331", "macd": 0.11, "kdj_j": 81.0}]

    monkeypatch.setattr("stock_cache.providers.tushare_adapter.TushareAdapter.fetch_instruments", fake_fetch_instruments)
    monkeypatch.setattr(
        "stock_cache.providers.tushare_adapter.TushareAdapter.fetch_recent_trade_dates",
        fake_fetch_recent_trade_dates,
    )
    monkeypatch.setattr("stock_cache.providers.tushare_adapter.TushareAdapter.fetch_daily", fake_fetch_daily)
    monkeypatch.setattr(
        "stock_cache.providers.tushare_adapter.TushareAdapter.fetch_daily_basic",
        fake_fetch_daily_basic,
    )
    monkeypatch.setattr(
        "stock_cache.providers.tushare_adapter.TushareAdapter.fetch_moneyflow",
        fake_fetch_moneyflow,
    )
    monkeypatch.setattr(
        "stock_cache.providers.tushare_adapter.TushareAdapter.fetch_indicators",
        fake_fetch_indicators,
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
                return market, indicator
        finally:
            async with pool.acquire() as connection:
                await connection.execute("delete from daily_indicators where ts_code = $1", ts_code)
                await connection.execute("delete from daily_market where ts_code = $1", ts_code)
            await pool.close()

    market, indicator = asyncio.run(verify())

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
