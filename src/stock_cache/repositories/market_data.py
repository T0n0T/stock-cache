from datetime import date, datetime

import asyncpg

from stock_cache.domain.models import DailyIndicatorRow, DailyMarketRow


def _coerce_date(value: object) -> date:
    if isinstance(value, date):
        return value
    if isinstance(value, datetime):
        return value.date()
    if isinstance(value, str):
        try:
            return date.fromisoformat(value)
        except ValueError:
            return datetime.strptime(value, "%Y%m%d").date()
    raise TypeError(f"unsupported trade_date value: {value!r}")


class MarketDataRepository:
    def __init__(self, pool: asyncpg.Pool) -> None:
        self._pool = pool

    async def upsert_daily_market(self, rows: list[DailyMarketRow]) -> None:
        if not rows:
            return
        sql, values = build_daily_market_upsert(rows)
        async with self._pool.acquire() as connection:
            await connection.executemany(sql, values)

    async def upsert_daily_indicators(self, rows: list[DailyIndicatorRow]) -> None:
        if not rows:
            return
        sql, values = build_daily_indicator_upsert(rows)
        async with self._pool.acquire() as connection:
            await connection.executemany(sql, values)

    async def fetch_raw(self, ts_code: str, start_date: str, end_date: str) -> dict[str, list[object]]:
        start = _coerce_date(start_date)
        end = _coerce_date(end_date)
        async with self._pool.acquire() as connection:
            market_rows = await connection.fetch(
                """
                select ts_code, trade_date, open, high, low, close, pct_chg,
                       turnover_rate, total_mv, net_mf_amount, source_provider
                from daily_market
                where ts_code = $1 and trade_date between $2 and $3
                order by trade_date
                """,
                ts_code,
                start,
                end,
            )
            indicator_rows = await connection.fetch(
                """
                select ts_code, trade_date, macd, macd_dif, macd_dea, kdj_k, kdj_d, kdj_j,
                       source_provider, source_interface, calc_fallback_used
                from daily_indicators
                where ts_code = $1 and trade_date between $2 and $3
                order by trade_date
                """,
                ts_code,
                start,
                end,
            )

        return {
            "market": [
                DailyMarketRow(
                    ts_code=row["ts_code"],
                    trade_date=_coerce_date(row["trade_date"]),
                    open=row["open"],
                    high=row["high"],
                    low=row["low"],
                    close=row["close"],
                    pct_chg=row["pct_chg"],
                    turnover_rate=row["turnover_rate"],
                    total_mv=row["total_mv"],
                    net_mf_amount=row["net_mf_amount"],
                    source_provider=row["source_provider"],
                )
                for row in market_rows
            ],
            "indicators": [
                DailyIndicatorRow(
                    ts_code=row["ts_code"],
                    trade_date=_coerce_date(row["trade_date"]),
                    macd=row["macd"],
                    macd_dif=row["macd_dif"],
                    macd_dea=row["macd_dea"],
                    kdj_k=row["kdj_k"],
                    kdj_d=row["kdj_d"],
                    kdj_j=row["kdj_j"],
                    source_provider=row["source_provider"],
                    source_interface=row["source_interface"],
                    calc_fallback_used=row["calc_fallback_used"],
                )
                for row in indicator_rows
            ],
        }

    async def screen(self, trade_date: str, filters: dict[str, object]) -> list[dict[str, object]]:
        where_clauses = ["dm.trade_date = $1"]
        values: list[object] = [_coerce_date(trade_date)]

        filter_map = {
            "pct_chg_gte": "dm.pct_chg >= ${index}",
            "turnover_rate_gte": "dm.turnover_rate >= ${index}",
            "total_mv_gte": "dm.total_mv >= ${index}",
            "total_mv_lte": "dm.total_mv <= ${index}",
            "macd_gte": "di.macd >= ${index}",
            "kdj_j_gte": "di.kdj_j >= ${index}",
        }
        for key, template in filter_map.items():
            if key in filters:
                values.append(filters[key])
                where_clauses.append(template.format(index=len(values)))

        sql = f"""
        select dm.ts_code,
               dm.trade_date,
               dm.pct_chg,
               dm.turnover_rate,
               dm.total_mv,
               di.macd,
               di.kdj_j
        from daily_market dm
        left join daily_indicators di
          on di.ts_code = dm.ts_code and di.trade_date = dm.trade_date
        where {' and '.join(where_clauses)}
        order by dm.ts_code
        """
        async with self._pool.acquire() as connection:
            rows = await connection.fetch(sql, *values)

        return [
            {
                "ts_code": row["ts_code"],
                "trade_date": _coerce_date(row["trade_date"]).isoformat(),
                "pct_chg": row["pct_chg"],
                "turnover_rate": row["turnover_rate"],
                "total_mv": row["total_mv"],
                "macd": row["macd"],
                "kdj_j": row["kdj_j"],
            }
            for row in rows
        ]


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
