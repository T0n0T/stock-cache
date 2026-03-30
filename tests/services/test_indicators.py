from datetime import date

from stock_cache.domain.models import DailyMarketRow
from stock_cache.services.indicators import calculate_macd_fallback


def test_calculate_macd_fallback_returns_rows_for_market_series() -> None:
    rows = [
        DailyMarketRow(ts_code="000001.SZ", trade_date=date(2026, 3, day), close=float(day))
        for day in range(1, 31)
    ]

    indicators = calculate_macd_fallback(rows)

    assert len(indicators) == 30
    assert indicators[-1].macd is not None
