from datetime import date

from stock_cache.domain.models import DailyIndicatorRow, DailyMarketRow
from stock_cache.repositories.market_data import (
    build_daily_indicator_upsert,
    build_daily_market_upsert,
)


def test_build_daily_market_upsert_uses_composite_key() -> None:
    row = DailyMarketRow(
        ts_code="000001.SZ",
        trade_date=date(2026, 3, 30),
        open=12.1,
        high=12.8,
        low=11.9,
        close=12.4,
        pct_chg=2.5,
        turnover_rate=3.4,
        total_mv=100.5,
        net_mf_amount=8.9,
        source_provider="tushare",
    )
    sql, values = build_daily_market_upsert([row])

    assert "ON CONFLICT (ts_code, trade_date)" in sql
    assert "VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)" in sql
    assert "SET open = EXCLUDED.open" in sql
    assert "high = EXCLUDED.high" in sql
    assert "low = EXCLUDED.low" in sql
    assert values == [
        (
            "000001.SZ",
            date(2026, 3, 30),
            12.1,
            12.8,
            11.9,
            12.4,
            2.5,
            3.4,
            100.5,
            8.9,
            "tushare",
        )
    ]


def test_build_daily_indicator_upsert_uses_composite_key() -> None:
    indicator = DailyIndicatorRow(
        ts_code="000001.SZ",
        trade_date=date(2026, 3, 30),
        macd=0.1,
        macd_dif=0.2,
        macd_dea=0.3,
        kdj_k=40.0,
        kdj_d=41.0,
        kdj_j=42.0,
        source_provider="tushare",
        source_interface="stk_factor",
        calc_fallback_used=True,
    )
    sql, values = build_daily_indicator_upsert([indicator])

    assert "ON CONFLICT (ts_code, trade_date)" in sql
    assert "VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)" in sql
    assert values == [
        (
            "000001.SZ",
            date(2026, 3, 30),
            0.1,
            0.2,
            0.3,
            40.0,
            41.0,
            42.0,
            "tushare",
            "stk_factor",
            True,
        )
    ]
