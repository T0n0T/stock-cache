import json
from datetime import date

from typer.testing import CliRunner

from stock_cache.cli import app
from stock_cache.db.pool import create_pool
from stock_cache.domain.models import DailyIndicatorRow, DailyMarketRow
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
