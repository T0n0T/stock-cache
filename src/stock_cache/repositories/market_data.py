from stock_cache.domain.models import DailyIndicatorRow, DailyMarketRow


def build_daily_market_upsert(rows: list[DailyMarketRow]) -> tuple[str, list[tuple[object, ...]]]:
    sql = """
    INSERT INTO daily_market (
        ts_code,
        trade_date,
        open,
        high,
        low,
        close,
        pct_chg,
        turnover_rate,
        total_mv,
        net_mf_amount,
        source_provider
    )
    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
    ON CONFLICT (ts_code, trade_date) DO UPDATE
    SET open = EXCLUDED.open,
        high = EXCLUDED.high,
        low = EXCLUDED.low,
        close = EXCLUDED.close,
        pct_chg = EXCLUDED.pct_chg,
        turnover_rate = EXCLUDED.turnover_rate,
        total_mv = EXCLUDED.total_mv,
        net_mf_amount = EXCLUDED.net_mf_amount,
        source_provider = EXCLUDED.source_provider,
        updated_at = NOW()
    """
    return sql, [
        (
            row.ts_code,
            row.trade_date,
            row.open,
            row.high,
            row.low,
            row.close,
            row.pct_chg,
            row.turnover_rate,
            row.total_mv,
            row.net_mf_amount,
            row.source_provider,
        )
        for row in rows
    ]


def build_daily_indicator_upsert(
    rows: list[DailyIndicatorRow],
) -> tuple[str, list[tuple[object, ...]]]:
    sql = """
    INSERT INTO daily_indicators (
        ts_code,
        trade_date,
        macd,
        macd_dif,
        macd_dea,
        kdj_k,
        kdj_d,
        kdj_j,
        source_provider,
        source_interface,
        calc_fallback_used
    )
    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
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
    return sql, [
        (
            row.ts_code,
            row.trade_date,
            row.macd,
            row.macd_dif,
            row.macd_dea,
            row.kdj_k,
            row.kdj_d,
            row.kdj_j,
            row.source_provider,
            row.source_interface,
            row.calc_fallback_used,
        )
        for row in rows
    ]
