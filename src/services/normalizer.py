from dataclasses import dataclass
from datetime import date, datetime

from domain.models import DailyIndicatorRow, DailyMarketRow


@dataclass(slots=True)
class NormalizedSymbolBundle:
    market_rows: list[DailyMarketRow]
    indicator_rows: list[DailyIndicatorRow]


def _parse_trade_date(value: str) -> date:
    return datetime.strptime(value, "%Y%m%d").date()


def _pick_value(payload: dict[str, object], *keys: str) -> object:
    for key in keys:
        value = payload.get(key)
        if value is not None:
            return value
    return None


def _merge_rows(
    rows: list[dict[str, object]],
    merged: dict[tuple[str, str], dict[str, object]],
    target_symbols: set[str] | None,
) -> None:
    for row in rows:
        ts_code = str(row["ts_code"])
        if target_symbols is not None and ts_code not in target_symbols:
            continue
        trade_date = str(row["trade_date"])
        merged.setdefault((ts_code, trade_date), {}).update(row)


def normalize_market_batches(
    daily_rows: list[dict[str, object]],
    daily_basic_rows: list[dict[str, object]],
    moneyflow_rows: list[dict[str, object]],
    adj_factor_rows: list[dict[str, object]],
    limit_rows: list[dict[str, object]],
    suspend_rows: list[dict[str, object]],
    indicator_rows: list[dict[str, object]],
    target_symbols: set[str] | None = None,
) -> NormalizedSymbolBundle:
    merged_market: dict[tuple[str, str], dict[str, object]] = {}
    for row_group in (daily_rows, daily_basic_rows, moneyflow_rows, adj_factor_rows, limit_rows, suspend_rows):
        _merge_rows(row_group, merged_market, target_symbols)

    merged_indicators: dict[tuple[str, str], dict[str, object]] = {}
    _merge_rows(indicator_rows, merged_indicators, target_symbols)

    market_core_fields = {
        "ts_code",
        "trade_date",
        "open",
        "high",
        "low",
        "close",
        "pre_close",
        "change",
        "pct_chg",
        "vol",
        "amount",
        "turnover_rate",
        "turnover_rate_f",
        "volume_ratio",
        "pe",
        "pe_ttm",
        "pb",
        "ps",
        "ps_ttm",
        "dv_ratio",
        "dv_ttm",
        "total_share",
        "float_share",
        "free_share",
        "total_mv",
        "circ_mv",
        "net_mf_vol",
        "net_mf_amount",
        "source_daily",
        "source_daily_basic",
        "source_moneyflow",
    }
    indicator_core_fields = {
        "ts_code",
        "trade_date",
        "macd",
        "macd_dif",
        "macd_dea",
        "macd_qfq",
        "macd_bfq",
        "macd_hfq",
        "macd_dif_qfq",
        "macd_dif_bfq",
        "macd_dif_hfq",
        "macd_dea_qfq",
        "macd_dea_bfq",
        "macd_dea_hfq",
        "kdj_k",
        "kdj_d",
        "kdj_j",
        "kdj_k_qfq",
        "kdj_k_bfq",
        "kdj_k_hfq",
        "kdj_d_qfq",
        "kdj_d_bfq",
        "kdj_d_hfq",
        "kdj_qfq",
        "kdj_bfq",
        "kdj_hfq",
        "rsi_6",
        "rsi_12",
        "rsi_24",
        "rsi_qfq_6",
        "rsi_bfq_6",
        "rsi_hfq_6",
        "rsi_qfq_12",
        "rsi_bfq_12",
        "rsi_hfq_12",
        "rsi_qfq_24",
        "rsi_bfq_24",
        "rsi_hfq_24",
        "boll_upper",
        "boll_mid",
        "boll_lower",
        "boll_upper_qfq",
        "boll_upper_bfq",
        "boll_upper_hfq",
        "boll_mid_qfq",
        "boll_mid_bfq",
        "boll_mid_hfq",
        "boll_lower_qfq",
        "boll_lower_bfq",
        "boll_lower_hfq",
        "cci",
        "cci_qfq",
        "cci_bfq",
        "cci_hfq",
        "source_interface",
        "calc_fallback_used",
        "open",
        "high",
        "low",
        "close",
        "pre_close",
        "change",
        "pct_change",
        "pct_chg",
        "vol",
        "amount",
        "turnover_rate",
        "turnover_rate_f",
        "volume_ratio",
        "pe",
        "pe_ttm",
        "pb",
        "ps",
        "ps_ttm",
        "dv_ratio",
        "dv_ttm",
        "total_share",
        "float_share",
        "free_share",
        "total_mv",
        "circ_mv",
    }

    market = [
        DailyMarketRow(
            ts_code=ts_code,
            trade_date=_parse_trade_date(trade_date),
            open=payload.get("open"),
            high=payload.get("high"),
            low=payload.get("low"),
            close=payload.get("close"),
            pre_close=payload.get("pre_close"),
            change=payload.get("change"),
            pct_chg=payload.get("pct_chg"),
            vol=payload.get("vol"),
            amount=payload.get("amount"),
            turnover_rate=payload.get("turnover_rate"),
            turnover_rate_f=payload.get("turnover_rate_f"),
            volume_ratio=payload.get("volume_ratio"),
            pe=payload.get("pe"),
            pe_ttm=payload.get("pe_ttm"),
            pb=payload.get("pb"),
            ps=payload.get("ps"),
            ps_ttm=payload.get("ps_ttm"),
            dv_ratio=payload.get("dv_ratio"),
            dv_ttm=payload.get("dv_ttm"),
            total_share=payload.get("total_share"),
            float_share=payload.get("float_share"),
            free_share=payload.get("free_share"),
            total_mv=payload.get("total_mv"),
            circ_mv=payload.get("circ_mv"),
            net_mf_vol=payload.get("net_mf_vol"),
            net_mf_amount=payload.get("net_mf_amount"),
            extra_market_jsonb={key: value for key, value in payload.items() if key not in market_core_fields},
            source_daily=payload.get("source_daily"),
            source_daily_basic=payload.get("source_daily_basic"),
            source_moneyflow=payload.get("source_moneyflow"),
        )
        for (ts_code, trade_date), payload in sorted(merged_market.items())
    ]
    indicators = [
        DailyIndicatorRow(
            ts_code=ts_code,
            trade_date=_parse_trade_date(trade_date),
            macd=_pick_value(payload, "macd", "macd_qfq", "macd_bfq", "macd_hfq"),
            macd_dif=_pick_value(payload, "macd_dif", "macd_dif_qfq", "macd_dif_bfq", "macd_dif_hfq"),
            macd_dea=_pick_value(payload, "macd_dea", "macd_dea_qfq", "macd_dea_bfq", "macd_dea_hfq"),
            kdj_k=_pick_value(payload, "kdj_k", "kdj_k_qfq", "kdj_k_bfq", "kdj_k_hfq"),
            kdj_d=_pick_value(payload, "kdj_d", "kdj_d_qfq", "kdj_d_bfq", "kdj_d_hfq"),
            kdj_j=_pick_value(payload, "kdj_j", "kdj_qfq", "kdj_bfq", "kdj_hfq"),
            rsi_6=_pick_value(payload, "rsi_6", "rsi_qfq_6", "rsi_bfq_6", "rsi_hfq_6"),
            rsi_12=_pick_value(payload, "rsi_12", "rsi_qfq_12", "rsi_bfq_12", "rsi_hfq_12"),
            rsi_24=_pick_value(payload, "rsi_24", "rsi_qfq_24", "rsi_bfq_24", "rsi_hfq_24"),
            boll_upper=_pick_value(payload, "boll_upper", "boll_upper_qfq", "boll_upper_bfq", "boll_upper_hfq"),
            boll_mid=_pick_value(payload, "boll_mid", "boll_mid_qfq", "boll_mid_bfq", "boll_mid_hfq"),
            boll_lower=_pick_value(payload, "boll_lower", "boll_lower_qfq", "boll_lower_bfq", "boll_lower_hfq"),
            cci=_pick_value(payload, "cci", "cci_qfq", "cci_bfq", "cci_hfq"),
            extra_factors_jsonb={key: value for key, value in payload.items() if key not in indicator_core_fields},
            source_interface=str(payload.get("source_interface") or "stk_factor"),
            calc_fallback_used=bool(payload.get("calc_fallback_used", False)),
        )
        for (ts_code, trade_date), payload in sorted(merged_indicators.items())
    ]
    return NormalizedSymbolBundle(market_rows=market, indicator_rows=indicators)


def normalize_symbol_bundle(
    ts_code: str,
    daily_rows: list[dict[str, object]],
    daily_basic_rows: list[dict[str, object]],
    moneyflow_rows: list[dict[str, object]],
    indicator_rows: list[dict[str, object]],
) -> NormalizedSymbolBundle:
    daily_rows = [{**row, "ts_code": ts_code} for row in daily_rows]
    daily_basic_rows = [{**row, "ts_code": ts_code} for row in daily_basic_rows]
    moneyflow_rows = [{**row, "ts_code": ts_code} for row in moneyflow_rows]
    indicator_rows = [{**row, "ts_code": ts_code} for row in indicator_rows]
    return normalize_market_batches(
        daily_rows=daily_rows,
        daily_basic_rows=daily_basic_rows,
        moneyflow_rows=moneyflow_rows,
        adj_factor_rows=[],
        limit_rows=[],
        suspend_rows=[],
        indicator_rows=indicator_rows,
        target_symbols={ts_code},
    )
