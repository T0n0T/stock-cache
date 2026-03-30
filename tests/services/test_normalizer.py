from datetime import date

from stock_cache.services.normalizer import normalize_symbol_bundle


def test_normalize_symbol_bundle_merges_rows_by_trade_date() -> None:
    result = normalize_symbol_bundle(
        ts_code="000001.SZ",
        daily_rows=[
            {
                "trade_date": "20260330",
                "open": 12.0,
                "high": 12.8,
                "low": 11.9,
                "close": 12.5,
                "pct_chg": 1.2,
            }
        ],
        daily_basic_rows=[{"trade_date": "20260330", "turnover_rate": 2.1, "total_mv": 1000.0}],
        moneyflow_rows=[{"trade_date": "20260330", "net_mf_amount": 12.3}],
        indicator_rows=[{"trade_date": "20260330", "macd": 0.1, "kdj_j": 80.0}],
    )

    assert len(result.market_rows) == 1
    assert result.market_rows[0].trade_date == date(2026, 3, 30)
    assert result.market_rows[0].open == 12.0
    assert result.market_rows[0].high == 12.8
    assert result.market_rows[0].low == 11.9
    assert result.market_rows[0].turnover_rate == 2.1
    assert result.indicator_rows[0].macd == 0.1
