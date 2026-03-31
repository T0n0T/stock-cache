from datetime import date

import pytest

from db.pool import create_pool
from domain.models import DailyIndicatorRow, DailyMarketRow
from repositories.market_data import MarketDataRepository


@pytest.mark.asyncio
async def test_market_data_repository_fetch_raw_returns_market_and_indicators(sample_dsn: str) -> None:
    pool = await create_pool(sample_dsn)
    repository = MarketDataRepository(pool)
    market_row = DailyMarketRow(
        ts_code="TESTRAW1.SZ",
        trade_date=date(2026, 3, 31),
        close=12.4,
        pct_chg=2.1,
        turnover_rate=3.5,
        total_mv=100.0,
        net_mf_amount=8.2,
        source_provider="integration-test",
    )
    indicator_row = DailyIndicatorRow(
        ts_code="TESTRAW1.SZ",
        trade_date=date(2026, 3, 31),
        macd=0.1,
        macd_dif=0.2,
        macd_dea=0.3,
        kdj_k=40.0,
        kdj_d=41.0,
        kdj_j=42.0,
        source_provider="integration-test",
        source_interface="integration",
        calc_fallback_used=True,
    )

    try:
        await repository.upsert_daily_market([market_row])
        await repository.upsert_daily_indicators([indicator_row])

        payload = await repository.fetch_raw(
            ts_code="TESTRAW1.SZ",
            start_date="2026-03-01",
            end_date="2026-03-31",
        )
    finally:
        async with pool.acquire() as connection:
            await connection.execute("delete from daily_indicators where ts_code = $1", "TESTRAW1.SZ")
            await connection.execute("delete from daily_market where ts_code = $1", "TESTRAW1.SZ")
        await pool.close()

    assert len(payload["market"]) == 1
    assert len(payload["indicators"]) == 1
    assert payload["market"][0].ts_code == "TESTRAW1.SZ"
    assert payload["market"][0].close == 12.4
    assert payload["indicators"][0].macd == 0.1


@pytest.mark.asyncio
async def test_market_data_repository_screen_filters_rows(sample_dsn: str) -> None:
    pool = await create_pool(sample_dsn)
    repository = MarketDataRepository(pool)
    market_row = DailyMarketRow(
        ts_code="TESTSCR1.SZ",
        trade_date=date(2026, 3, 31),
        close=18.8,
        pct_chg=6.5,
        turnover_rate=4.2,
        total_mv=28000000000.0,
        source_provider="integration-test",
    )
    indicator_row = DailyIndicatorRow(
        ts_code="TESTSCR1.SZ",
        trade_date=date(2026, 3, 31),
        macd=0.13,
        kdj_j=91.4,
        source_provider="integration-test",
        source_interface="integration",
    )

    try:
        await repository.upsert_daily_market([market_row])
        await repository.upsert_daily_indicators([indicator_row])

        rows = await repository.screen(
            trade_date="2026-03-31",
            filters={
                "pct_chg_gte": 5,
                "turnover_rate_gte": 3,
                "macd_gte": 0,
            },
        )
    finally:
        async with pool.acquire() as connection:
            await connection.execute("delete from daily_indicators where ts_code = $1", "TESTSCR1.SZ")
            await connection.execute("delete from daily_market where ts_code = $1", "TESTSCR1.SZ")
        await pool.close()

    assert len(rows) == 1
    assert rows[0]["ts_code"] == "TESTSCR1.SZ"
    assert rows[0]["pct_chg"] == 6.5
    assert rows[0]["turnover_rate"] == 4.2
    assert rows[0]["macd"] == 0.13
