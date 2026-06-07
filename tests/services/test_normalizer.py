from datetime import date

from services.normalizer import normalize_market_batches


def test_normalize_market_batches_merges_bulk_rows_by_symbol_and_trade_date() -> None:
    result = normalize_market_batches(
        daily_rows=[
            {
                "ts_code": "000001.SZ",
                "trade_date": "20260330",
                "open": 12.0,
                "high": 12.8,
                "low": 11.9,
                "close": 12.5,
                "pre_close": 12.1,
                "change": 0.4,
                "pct_chg": 1.2,
                "vol": 123456.0,
                "amount": 234567.0,
                "source_daily": "daily",
            }
        ],
        daily_basic_rows=[
            {
                "ts_code": "000001.SZ",
                "trade_date": "20260330",
                "turnover_rate": 2.1,
                "turnover_rate_f": 1.8,
                "volume_ratio": 0.9,
                "pe": 10.0,
                "pe_ttm": 11.0,
                "pb": 1.2,
                "ps": 2.3,
                "ps_ttm": 2.4,
                "dv_ratio": 0.5,
                "dv_ttm": 0.6,
                "total_share": 10000.0,
                "float_share": 8000.0,
                "free_share": 7000.0,
                "total_mv": 1000.0,
                "circ_mv": 800.0,
                "source_daily_basic": "daily_basic",
            }
        ],
        moneyflow_rows=[
            {
                "ts_code": "000001.SZ",
                "trade_date": "20260330",
                "net_mf_vol": 11.1,
                "net_mf_amount": 12.3,
                "buy_sm_vol": 10,
                "sell_sm_vol": 8,
                "source_moneyflow": "moneyflow",
            }
        ],
        adj_factor_rows=[
            {
                "ts_code": "000001.SZ",
                "trade_date": "20260330",
                "adj_factor": 123.4,
            }
        ],
        limit_rows=[
            {
                "ts_code": "000001.SZ",
                "trade_date": "20260330",
                "up_limit": 13.75,
                "down_limit": 11.25,
            }
        ],
        suspend_rows=[
            {
                "ts_code": "000001.SZ",
                "trade_date": "20260330",
                "suspend_type": "R",
                "suspend_timing": None,
            }
        ],
        indicator_rows=[
            {
                "ts_code": "000001.SZ",
                "trade_date": "20260330",
                "macd": 0.1,
                "macd_dif": 0.2,
                "macd_dea": 0.3,
                "kdj_k": 70.0,
                "kdj_d": 60.0,
                "kdj_j": 90.0,
                "rsi_6": 55.0,
                "rsi_12": 50.0,
                "rsi_24": 48.0,
                "boll_upper": 13.0,
                "boll_mid": 12.0,
                "boll_lower": 11.0,
                "cci": 101.0,
                "ema_qfq_5": 12.2,
                "wr_bfq": 17.1,
                "source_interface": "stk_factor_pro",
            }
        ],
    )

    assert len(result.market_rows) == 1
    assert result.market_rows[0].trade_date == date(2026, 3, 30)
    assert result.market_rows[0].open == 12.0
    assert result.market_rows[0].high == 12.8
    assert result.market_rows[0].low == 11.9
    assert result.market_rows[0].pre_close == 12.1
    assert result.market_rows[0].change == 0.4
    assert result.market_rows[0].vol == 123456.0
    assert result.market_rows[0].amount == 234567.0
    assert result.market_rows[0].turnover_rate == 2.1
    assert result.market_rows[0].turnover_rate_f == 1.8
    assert result.market_rows[0].volume_ratio == 0.9
    assert result.market_rows[0].pe == 10.0
    assert result.market_rows[0].pe_ttm == 11.0
    assert result.market_rows[0].pb == 1.2
    assert result.market_rows[0].ps == 2.3
    assert result.market_rows[0].ps_ttm == 2.4
    assert result.market_rows[0].dv_ratio == 0.5
    assert result.market_rows[0].dv_ttm == 0.6
    assert result.market_rows[0].total_share == 10000.0
    assert result.market_rows[0].float_share == 8000.0
    assert result.market_rows[0].free_share == 7000.0
    assert result.market_rows[0].total_mv == 1000.0
    assert result.market_rows[0].circ_mv == 800.0
    assert result.market_rows[0].net_mf_vol == 11.1
    assert result.market_rows[0].net_mf_amount == 12.3
    assert result.market_rows[0].extra_market_jsonb == {
        "adj_factor": 123.4,
        "buy_sm_vol": 10,
        "down_limit": 11.25,
        "sell_sm_vol": 8,
        "suspend_timing": None,
        "suspend_type": "R",
        "up_limit": 13.75,
    }
    assert result.indicator_rows[0].macd == 0.1
    assert result.indicator_rows[0].rsi_6 == 55.0
    assert result.indicator_rows[0].rsi_12 == 50.0
    assert result.indicator_rows[0].rsi_24 == 48.0
    assert result.indicator_rows[0].boll_upper == 13.0
    assert result.indicator_rows[0].boll_mid == 12.0
    assert result.indicator_rows[0].boll_lower == 11.0
    assert result.indicator_rows[0].cci == 101.0
    assert result.indicator_rows[0].source_interface == "stk_factor_pro"
    assert result.indicator_rows[0].extra_factors_jsonb == {
        "ema_qfq_5": 12.2,
        "wr_bfq": 17.1,
    }


def test_normalize_market_batches_builds_cyq_rows() -> None:
    result = normalize_market_batches(
        daily_rows=[],
        daily_basic_rows=[],
        moneyflow_rows=[],
        adj_factor_rows=[],
        limit_rows=[],
        suspend_rows=[],
        indicator_rows=[],
        cyq_chips_rows=[
            {
                "ts_code": "000001.SZ",
                "trade_date": "20260330",
                "price": 12.34,
                "percent": 0.56,
                "source_interface": "cyq_chips",
                "extra_bucket": "kept",
            }
        ],
        cyq_perf_rows=[
            {
                "ts_code": "000001.SZ",
                "trade_date": "20260330",
                "his_low": 8.1,
                "his_high": 15.2,
                "cost_5pct": 9.3,
                "cost_15pct": 10.4,
                "cost_50pct": 11.5,
                "cost_85pct": 12.6,
                "cost_95pct": 13.7,
                "weight_avg": 11.8,
                "winner_rate": 0.72,
                "source_interface": "cyq_perf",
                "extra_metric": "kept",
            }
        ],
    )

    assert len(result.cyq_chips_rows) == 1
    chip = result.cyq_chips_rows[0]
    assert chip.ts_code == "000001.SZ"
    assert chip.trade_date == date(2026, 3, 30)
    assert chip.price == 12.34
    assert chip.percent == 0.56
    assert chip.source_interface == "cyq_chips"
    assert chip.extra_chips_jsonb == {"extra_bucket": "kept"}
    assert len(result.cyq_perf_rows) == 1
    perf = result.cyq_perf_rows[0]
    assert perf.cost_50pct == 11.5
    assert perf.winner_rate == 0.72
    assert perf.source_interface == "cyq_perf"
    assert perf.extra_perf_jsonb == {"extra_metric": "kept"}


def test_normalize_market_batches_maps_stk_factor_pro_suffix_fields_to_core_indicator_columns() -> None:
    result = normalize_market_batches(
        daily_rows=[],
        daily_basic_rows=[],
        moneyflow_rows=[],
        adj_factor_rows=[],
        limit_rows=[],
        suspend_rows=[],
        indicator_rows=[
            {
                "ts_code": "000001.SZ",
                "trade_date": "20260330",
                "macd_dif_qfq": 0.21,
                "macd_dea_qfq": 0.12,
                "macd_qfq": 0.18,
                "kdj_k_qfq": 78.0,
                "kdj_d_qfq": 70.0,
                "kdj_qfq": 94.0,
                "rsi_qfq_6": 55.0,
                "rsi_qfq_12": 50.0,
                "rsi_qfq_24": 48.0,
                "boll_upper_qfq": 13.0,
                "boll_mid_qfq": 12.0,
                "boll_lower_qfq": 11.0,
                "cci_qfq": 101.0,
                "wr_bfq": 17.1,
                "source_interface": "stk_factor_pro",
            }
        ],
    )

    assert len(result.indicator_rows) == 1
    indicator = result.indicator_rows[0]
    assert indicator.macd_dif == 0.21
    assert indicator.macd_dea == 0.12
    assert indicator.macd == 0.18
    assert indicator.kdj_k == 78.0
    assert indicator.kdj_d == 70.0
    assert indicator.kdj_j == 94.0
    assert indicator.rsi_6 == 55.0
    assert indicator.rsi_12 == 50.0
    assert indicator.rsi_24 == 48.0
    assert indicator.boll_upper == 13.0
    assert indicator.boll_mid == 12.0
    assert indicator.boll_lower == 11.0
    assert indicator.cci == 101.0
    assert indicator.extra_factors_jsonb == {"wr_bfq": 17.1}
