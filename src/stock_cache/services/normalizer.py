from dataclasses import dataclass
from datetime import datetime

from stock_cache.domain.models import DailyIndicatorRow, DailyMarketRow


@dataclass(slots=True)
class NormalizedSymbolBundle:
    market_rows: list[DailyMarketRow]
    indicator_rows: list[DailyIndicatorRow]


def _parse_trade_date(value: str) -> datetime.date:
    return datetime.strptime(value, "%Y%m%d").date()


def normalize_symbol_bundle(
    ts_code: str,
    daily_rows: list[dict[str, object]],
    daily_basic_rows: list[dict[str, object]],
    moneyflow_rows: list[dict[str, object]],
    indicator_rows: list[dict[str, object]],
) -> NormalizedSymbolBundle:
    merged: dict[str, dict[str, object]] = {}
    for row_group in (daily_rows, daily_basic_rows, moneyflow_rows):
        for row in row_group:
            merged.setdefault(str(row["trade_date"]), {}).update(row)
    indicators_by_date = {str(row["trade_date"]): row for row in indicator_rows}

    market = [
        DailyMarketRow(
            ts_code=ts_code,
            trade_date=_parse_trade_date(trade_date),
            close=payload.get("close"),
            pct_chg=payload.get("pct_chg"),
            turnover_rate=payload.get("turnover_rate"),
            total_mv=payload.get("total_mv"),
            net_mf_amount=payload.get("net_mf_amount"),
        )
        for trade_date, payload in sorted(merged.items())
    ]
    indicators = [
        DailyIndicatorRow(
            ts_code=ts_code,
            trade_date=_parse_trade_date(trade_date),
            macd=payload.get("macd"),
            macd_dif=payload.get("macd_dif"),
            macd_dea=payload.get("macd_dea"),
            kdj_k=payload.get("kdj_k"),
            kdj_d=payload.get("kdj_d"),
            kdj_j=payload.get("kdj_j"),
        )
        for trade_date, payload in sorted(indicators_by_date.items())
    ]
    return NormalizedSymbolBundle(market_rows=market, indicator_rows=indicators)
