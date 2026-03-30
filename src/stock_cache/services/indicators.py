from stock_cache.domain.models import DailyIndicatorRow, DailyMarketRow


def calculate_macd_fallback(rows: list[DailyMarketRow]) -> list[DailyIndicatorRow]:
    ema12 = None
    ema26 = None
    dea = 0.0
    results: list[DailyIndicatorRow] = []
    for row in rows:
        if row.close is None:
            raise ValueError(
                f"close price is required for MACD fallback: {row.ts_code} on {row.trade_date.isoformat()}"
            )
        close = row.close
        ema12 = close if ema12 is None else (close * 2 / 13) + ema12 * (11 / 13)
        ema26 = close if ema26 is None else (close * 2 / 27) + ema26 * (25 / 27)
        dif = ema12 - ema26
        dea = dea * (8 / 10) + dif * (2 / 10)
        macd = (dif - dea) * 2
        results.append(
            DailyIndicatorRow(
                ts_code=row.ts_code,
                trade_date=row.trade_date,
                macd_dif=dif,
                macd_dea=dea,
                macd=macd,
                calc_fallback_used=True,
                source_provider="local",
                source_interface="macd_fallback",
            )
        )
    return results
