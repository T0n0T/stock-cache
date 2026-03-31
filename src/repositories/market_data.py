from datetime import date, datetime
import json
import math

import asyncpg

from domain.models import DailyIndicatorRow, DailyMarketRow


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


def _json_ready(value: object) -> object:
    if isinstance(value, dict):
        return {str(key): _json_ready(item) for key, item in value.items()}
    if isinstance(value, list):
        return [_json_ready(item) for item in value]
    if isinstance(value, float) and (math.isnan(value) or math.isinf(value)):
        return None
    return value


def _decode_jsonb(value: object) -> object:
    if value is None:
        return {}
    if isinstance(value, str):
        return json.loads(value)
    return value


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
                       pre_close, change, vol, amount,
                       turnover_rate, turnover_rate_f, volume_ratio,
                       pe, pe_ttm, pb, ps, ps_ttm, dv_ratio, dv_ttm,
                       total_share, float_share, free_share,
                       total_mv, circ_mv, net_mf_vol, net_mf_amount, extra_market_jsonb,
                       source_provider, source_daily, source_daily_basic, source_moneyflow
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
                       rsi_6, rsi_12, rsi_24, boll_upper, boll_mid, boll_lower, cci,
                       extra_factors_jsonb, source_provider, source_interface, calc_fallback_used
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
                    pre_close=row["pre_close"],
                    change=row["change"],
                    pct_chg=row["pct_chg"],
                    vol=row["vol"],
                    amount=row["amount"],
                    turnover_rate=row["turnover_rate"],
                    turnover_rate_f=row["turnover_rate_f"],
                    volume_ratio=row["volume_ratio"],
                    pe=row["pe"],
                    pe_ttm=row["pe_ttm"],
                    pb=row["pb"],
                    ps=row["ps"],
                    ps_ttm=row["ps_ttm"],
                    dv_ratio=row["dv_ratio"],
                    dv_ttm=row["dv_ttm"],
                    total_share=row["total_share"],
                    float_share=row["float_share"],
                    free_share=row["free_share"],
                    total_mv=row["total_mv"],
                    circ_mv=row["circ_mv"],
                    net_mf_vol=row["net_mf_vol"],
                    net_mf_amount=row["net_mf_amount"],
                    extra_market_jsonb=_decode_jsonb(row["extra_market_jsonb"]),
                    source_provider=row["source_provider"],
                    source_daily=row["source_daily"],
                    source_daily_basic=row["source_daily_basic"],
                    source_moneyflow=row["source_moneyflow"],
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
                    rsi_6=row["rsi_6"],
                    rsi_12=row["rsi_12"],
                    rsi_24=row["rsi_24"],
                    boll_upper=row["boll_upper"],
                    boll_mid=row["boll_mid"],
                    boll_lower=row["boll_lower"],
                    cci=row["cci"],
                    extra_factors_jsonb=_decode_jsonb(row["extra_factors_jsonb"]),
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
        pre_close,
        change,
        pct_chg,
        vol,
        amount,
        turnover_rate,
        turnover_rate_f,
        volume_ratio,
        pe,
        pe_ttm,
        pb,
        ps,
        ps_ttm,
        dv_ratio,
        dv_ttm,
        total_share,
        float_share,
        free_share,
        total_mv,
        circ_mv,
        net_mf_vol,
        net_mf_amount,
        extra_market_jsonb,
        source_provider,
        source_daily,
        source_daily_basic,
        source_moneyflow
    )
    VALUES (
        $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16,
        $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29, $30,
        $31, $32, $33
    )
    ON CONFLICT (ts_code, trade_date) DO UPDATE
    SET open = EXCLUDED.open,
        high = EXCLUDED.high,
        low = EXCLUDED.low,
        close = EXCLUDED.close,
        pre_close = EXCLUDED.pre_close,
        change = EXCLUDED.change,
        pct_chg = EXCLUDED.pct_chg,
        vol = EXCLUDED.vol,
        amount = EXCLUDED.amount,
        turnover_rate = EXCLUDED.turnover_rate,
        turnover_rate_f = EXCLUDED.turnover_rate_f,
        volume_ratio = EXCLUDED.volume_ratio,
        pe = EXCLUDED.pe,
        pe_ttm = EXCLUDED.pe_ttm,
        pb = EXCLUDED.pb,
        ps = EXCLUDED.ps,
        ps_ttm = EXCLUDED.ps_ttm,
        dv_ratio = EXCLUDED.dv_ratio,
        dv_ttm = EXCLUDED.dv_ttm,
        total_share = EXCLUDED.total_share,
        float_share = EXCLUDED.float_share,
        free_share = EXCLUDED.free_share,
        total_mv = EXCLUDED.total_mv,
        circ_mv = EXCLUDED.circ_mv,
        net_mf_vol = EXCLUDED.net_mf_vol,
        net_mf_amount = EXCLUDED.net_mf_amount,
        extra_market_jsonb = EXCLUDED.extra_market_jsonb,
        source_provider = EXCLUDED.source_provider,
        source_daily = EXCLUDED.source_daily,
        source_daily_basic = EXCLUDED.source_daily_basic,
        source_moneyflow = EXCLUDED.source_moneyflow,
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
            row.pre_close,
            row.change,
            row.pct_chg,
            row.vol,
            row.amount,
            row.turnover_rate,
            row.turnover_rate_f,
            row.volume_ratio,
            row.pe,
            row.pe_ttm,
            row.pb,
            row.ps,
            row.ps_ttm,
            row.dv_ratio,
            row.dv_ttm,
            row.total_share,
            row.float_share,
            row.free_share,
            row.total_mv,
            row.circ_mv,
            row.net_mf_vol,
            row.net_mf_amount,
            json.dumps(_json_ready(row.extra_market_jsonb), ensure_ascii=False),
            row.source_provider,
            row.source_daily,
            row.source_daily_basic,
            row.source_moneyflow,
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
        rsi_6,
        rsi_12,
        rsi_24,
        boll_upper,
        boll_mid,
        boll_lower,
        cci,
        extra_factors_jsonb,
        source_provider,
        source_interface,
        calc_fallback_used
    )
    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)
    ON CONFLICT (ts_code, trade_date) DO UPDATE
    SET macd = EXCLUDED.macd,
        macd_dif = EXCLUDED.macd_dif,
        macd_dea = EXCLUDED.macd_dea,
        kdj_k = EXCLUDED.kdj_k,
        kdj_d = EXCLUDED.kdj_d,
        kdj_j = EXCLUDED.kdj_j,
        rsi_6 = EXCLUDED.rsi_6,
        rsi_12 = EXCLUDED.rsi_12,
        rsi_24 = EXCLUDED.rsi_24,
        boll_upper = EXCLUDED.boll_upper,
        boll_mid = EXCLUDED.boll_mid,
        boll_lower = EXCLUDED.boll_lower,
        cci = EXCLUDED.cci,
        extra_factors_jsonb = EXCLUDED.extra_factors_jsonb,
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
            row.rsi_6,
            row.rsi_12,
            row.rsi_24,
            row.boll_upper,
            row.boll_mid,
            row.boll_lower,
            row.cci,
            json.dumps(_json_ready(row.extra_factors_jsonb), ensure_ascii=False),
            row.source_provider,
            row.source_interface,
            row.calc_fallback_used,
        )
        for row in rows
    ]
