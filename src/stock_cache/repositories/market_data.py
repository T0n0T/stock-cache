from dataclasses import asdict

from stock_cache.domain.models import DailyIndicatorRow, DailyMarketRow


def build_daily_market_upsert(rows: list[DailyMarketRow]) -> tuple[str, list[dict[str, object]]]:
    sql = """
    INSERT INTO daily_market (ts_code, trade_date, close, pct_chg, turnover_rate, total_mv, net_mf_amount, source_provider)
    VALUES (:ts_code, :trade_date, :close, :pct_chg, :turnover_rate, :total_mv, :net_mf_amount, :source_provider)
    ON CONFLICT (ts_code, trade_date) DO UPDATE
    SET close = EXCLUDED.close,
        pct_chg = EXCLUDED.pct_chg,
        turnover_rate = EXCLUDED.turnover_rate,
        total_mv = EXCLUDED.total_mv,
        net_mf_amount = EXCLUDED.net_mf_amount,
        source_provider = EXCLUDED.source_provider,
        updated_at = NOW()
    """
    return sql, [asdict(row) for row in rows]


def build_daily_indicator_upsert(
    rows: list[DailyIndicatorRow],
) -> tuple[str, list[dict[str, object]]]:
    sql = """
    INSERT INTO daily_indicators (ts_code, trade_date, macd, macd_dif, macd_dea, kdj_k, kdj_d, kdj_j, source_provider, source_interface, calc_fallback_used)
    VALUES (:ts_code, :trade_date, :macd, :macd_dif, :macd_dea, :kdj_k, :kdj_d, :kdj_j, :source_provider, :source_interface, :calc_fallback_used)
    ON CONFLICT (ts_code, trade_date) DO UPDATE
    SET macd = EXCLUDED.macd,
        macd_dif = EXCLUDED.macd_dif,
        macd_dea = EXCLUDED.macd_dea,
        kdj_k = EXCLUDED.kdj_k,
        kdj_d = EXCLUDED.kdj_d,
        kdj_j = EXCLUDED.kdj_j,
        source_provider = EXCLUDED.source_provider,
        source_interface = EXCLUDED.source_interface,
        calc_fallback_used = EXCLUDED.calc_fallback_used,
        updated_at = NOW()
    """
    return sql, [asdict(row) for row in rows]
