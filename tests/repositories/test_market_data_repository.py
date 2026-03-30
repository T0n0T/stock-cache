from datetime import date

from stock_cache.domain.models import DailyIndicatorRow, DailyMarketRow
from stock_cache.repositories.market_data import (
    build_daily_indicator_upsert,
    build_daily_market_upsert,
)


def test_build_daily_market_upsert_uses_composite_key() -> None:
    row = DailyMarketRow(ts_code="000001.SZ", trade_date=date(2026, 3, 30), close=12.4)
    sql, values = build_daily_market_upsert([row])

    assert "ON CONFLICT (ts_code, trade_date)" in sql
    assert values[0]["ts_code"] == "000001.SZ"


def test_build_daily_indicator_upsert_uses_composite_key() -> None:
    indicator = DailyIndicatorRow(ts_code="000001.SZ", trade_date=date(2026, 3, 30), macd=0.1)
    sql, values = build_daily_indicator_upsert([indicator])

    assert "ON CONFLICT (ts_code, trade_date)" in sql
    assert values[0]["macd"] == 0.1
