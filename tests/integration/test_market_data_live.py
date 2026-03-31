from datetime import date

import pytest

from stock_cache.db.pool import create_pool
from stock_cache.domain.models import DailyIndicatorRow, DailyMarketRow
from stock_cache.repositories.market_data import (
    build_daily_indicator_upsert,
    build_daily_market_upsert,
)


@pytest.mark.asyncio
async def test_live_daily_market_upsert_persists_row(sample_dsn: str) -> None:
    pool = await create_pool(sample_dsn)
    row = DailyMarketRow(
        ts_code="TEST0001.SZ",
        trade_date=date(2026, 3, 31),
        open=10.0,
        high=10.8,
        low=9.9,
        close=10.5,
        pct_chg=1.5,
        turnover_rate=2.5,
        total_mv=123.4,
        net_mf_amount=5.6,
        source_provider="integration-test",
    )
    sql, values = build_daily_market_upsert([row])

    async with pool.acquire() as connection:
        await connection.execute(
            "delete from daily_market where ts_code = $1 and trade_date = $2",
            row.ts_code,
            row.trade_date,
        )
        try:
            await connection.executemany(sql, values)
            persisted = await connection.fetchrow(
                """
                select ts_code, trade_date, close, pct_chg, turnover_rate, total_mv, net_mf_amount, source_provider
                from daily_market
                where ts_code = $1 and trade_date = $2
                """,
                row.ts_code,
                row.trade_date,
            )
        finally:
            await connection.execute(
                "delete from daily_market where ts_code = $1 and trade_date = $2",
                row.ts_code,
                row.trade_date,
            )

    await pool.close()

    assert persisted is not None
    assert persisted["ts_code"] == row.ts_code
    assert persisted["trade_date"] == row.trade_date
    assert persisted["close"] == row.close
    assert persisted["pct_chg"] == row.pct_chg
    assert persisted["turnover_rate"] == row.turnover_rate
    assert persisted["total_mv"] == row.total_mv
    assert persisted["net_mf_amount"] == row.net_mf_amount
    assert persisted["source_provider"] == row.source_provider


@pytest.mark.asyncio
async def test_live_daily_indicator_upsert_persists_row(sample_dsn: str) -> None:
    pool = await create_pool(sample_dsn)
    row = DailyIndicatorRow(
        ts_code="TEST0001.SZ",
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
    sql, values = build_daily_indicator_upsert([row])

    async with pool.acquire() as connection:
        await connection.execute(
            "delete from daily_indicators where ts_code = $1 and trade_date = $2",
            row.ts_code,
            row.trade_date,
        )
        try:
            await connection.executemany(sql, values)
            persisted = await connection.fetchrow(
                """
                select ts_code, trade_date, macd, macd_dif, macd_dea, kdj_k, kdj_d, kdj_j,
                       source_provider, source_interface, calc_fallback_used
                from daily_indicators
                where ts_code = $1 and trade_date = $2
                """,
                row.ts_code,
                row.trade_date,
            )
        finally:
            await connection.execute(
                "delete from daily_indicators where ts_code = $1 and trade_date = $2",
                row.ts_code,
                row.trade_date,
            )

    await pool.close()

    assert persisted is not None
    assert persisted["ts_code"] == row.ts_code
    assert persisted["trade_date"] == row.trade_date
    assert persisted["macd"] == row.macd
    assert persisted["macd_dif"] == row.macd_dif
    assert persisted["macd_dea"] == row.macd_dea
    assert persisted["kdj_k"] == row.kdj_k
    assert persisted["kdj_d"] == row.kdj_d
    assert persisted["kdj_j"] == row.kdj_j
    assert persisted["source_provider"] == row.source_provider
    assert persisted["source_interface"] == row.source_interface
    assert persisted["calc_fallback_used"] is row.calc_fallback_used
