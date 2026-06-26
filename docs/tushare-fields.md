# Tushare Fields

This file is generated from `schemas/tushare-fields.yaml`.
Do not edit it by hand; update the YAML and rerun `tools/generate_tushare_fields_doc.py`.

## stock_stk_factor_pro

- API: `stk_factor_pro`
- Official doc: https://tushare.pro/wctapi/documents/328.md
- Checked at: `2026-06-26`
- SDK version: `1.4.29`
- Fetch plan: `kind`=`by_trade_date`; `params`=`trade_date`
- Write strategy: `type`=`upsert`; `key`=`trade_date`, `ts_code`
- Note: Fields are frozen from official Tushare output docs and SDK samples checked on 2026-06-26; optional output fields are included and should be requested explicitly with the Tushare fields parameter. pg_type is chosen explicitly from official docs plus source semantics, not inferred mechanically from pandas dtype.
- Note: Official document id 328 was checked; SDK sample used 000001.SZ over 20260529-20260626.

| Field | PostgreSQL type | Nullable | Source format | Description |
| --- | --- | --- | --- | --- |
| `ts_code` | `text` | no |  | 股票代码 |
| `trade_date` | `date` | no | YYYYMMDD | 交易日期 |
| `open` | `double precision` | yes |  | 开盘价 |
| `open_hfq` | `double precision` | yes |  | 开盘价（后复权） |
| `open_qfq` | `double precision` | yes |  | 开盘价（前复权） |
| `high` | `double precision` | yes |  | 最高价 |
| `high_hfq` | `double precision` | yes |  | 最高价（后复权） |
| `high_qfq` | `double precision` | yes |  | 最高价（前复权） |
| `low` | `double precision` | yes |  | 最低价 |
| `low_hfq` | `double precision` | yes |  | 最低价（后复权） |
| `low_qfq` | `double precision` | yes |  | 最低价（前复权） |
| `close` | `double precision` | yes |  | 收盘价 |
| `close_hfq` | `double precision` | yes |  | 收盘价（后复权） |
| `close_qfq` | `double precision` | yes |  | 收盘价（前复权） |
| `pre_close` | `double precision` | yes |  | 昨收价(前复权)--为daily接口的pre_close,以当时复权因子计算值跟前一日close_qfq对不上，可不用 |
| `change` | `double precision` | yes |  | 涨跌额 |
| `pct_chg` | `double precision` | yes |  | 涨跌幅 （除权后的涨跌幅） |
| `vol` | `double precision` | yes |  | 成交量 （手） |
| `amount` | `double precision` | yes |  | 成交额 （千元） |
| `turnover_rate` | `double precision` | yes |  | 换手率（%） |
| `turnover_rate_f` | `double precision` | yes |  | 换手率（自由流通股） |
| `volume_ratio` | `double precision` | yes |  | 量比 |
| `pe` | `double precision` | yes |  | 市盈率（总市值/净利润， 亏损的PE为空） |
| `pe_ttm` | `double precision` | yes |  | 市盈率（TTM，亏损的PE为空） |
| `pb` | `double precision` | yes |  | 市净率（总市值/净资产） |
| `ps` | `double precision` | yes |  | 市销率 |
| `ps_ttm` | `double precision` | yes |  | 市销率（TTM） |
| `dv_ratio` | `double precision` | yes |  | 股息率 （%） |
| `dv_ttm` | `double precision` | yes |  | 股息率（TTM）（%） |
| `total_share` | `double precision` | yes |  | 总股本 （万股） |
| `float_share` | `double precision` | yes |  | 流通股本 （万股） |
| `free_share` | `double precision` | yes |  | 自由流通股本 （万） |
| `total_mv` | `double precision` | yes |  | 总市值 （万元） |
| `circ_mv` | `double precision` | yes |  | 流通市值（万元） |
| `adj_factor` | `double precision` | yes |  | 复权因子 |
| `asi_bfq` | `double precision` | yes |  | 振动升降指标-OPEN, CLOSE, HIGH, LOW, M1=26, M2=10 |
| `asi_hfq` | `double precision` | yes |  | 振动升降指标-OPEN, CLOSE, HIGH, LOW, M1=26, M2=10 |
| `asi_qfq` | `double precision` | yes |  | 振动升降指标-OPEN, CLOSE, HIGH, LOW, M1=26, M2=10 |
| `asit_bfq` | `double precision` | yes |  | 振动升降指标-OPEN, CLOSE, HIGH, LOW, M1=26, M2=10 |
| `asit_hfq` | `double precision` | yes |  | 振动升降指标-OPEN, CLOSE, HIGH, LOW, M1=26, M2=10 |
| `asit_qfq` | `double precision` | yes |  | 振动升降指标-OPEN, CLOSE, HIGH, LOW, M1=26, M2=10 |
| `atr_bfq` | `double precision` | yes |  | 真实波动N日平均值-CLOSE, HIGH, LOW, N=20 |
| `atr_hfq` | `double precision` | yes |  | 真实波动N日平均值-CLOSE, HIGH, LOW, N=20 |
| `atr_qfq` | `double precision` | yes |  | 真实波动N日平均值-CLOSE, HIGH, LOW, N=20 |
| `bbi_bfq` | `double precision` | yes |  | BBI多空指标-CLOSE, M1=3, M2=6, M3=12, M4=20 |
| `bbi_hfq` | `double precision` | yes |  | BBI多空指标-CLOSE, M1=3, M2=6, M3=12, M4=21 |
| `bbi_qfq` | `double precision` | yes |  | BBI多空指标-CLOSE, M1=3, M2=6, M3=12, M4=22 |
| `bias1_bfq` | `double precision` | yes |  | BIAS乖离率-CLOSE, L1=6, L2=12, L3=24 |
| `bias1_hfq` | `double precision` | yes |  | BIAS乖离率-CLOSE, L1=6, L2=12, L3=24 |
| `bias1_qfq` | `double precision` | yes |  | BIAS乖离率-CLOSE, L1=6, L2=12, L3=24 |
| `bias2_bfq` | `double precision` | yes |  | BIAS乖离率-CLOSE, L1=6, L2=12, L3=24 |
| `bias2_hfq` | `double precision` | yes |  | BIAS乖离率-CLOSE, L1=6, L2=12, L3=24 |
| `bias2_qfq` | `double precision` | yes |  | BIAS乖离率-CLOSE, L1=6, L2=12, L3=24 |
| `bias3_bfq` | `double precision` | yes |  | BIAS乖离率-CLOSE, L1=6, L2=12, L3=24 |
| `bias3_hfq` | `double precision` | yes |  | BIAS乖离率-CLOSE, L1=6, L2=12, L3=24 |
| `bias3_qfq` | `double precision` | yes |  | BIAS乖离率-CLOSE, L1=6, L2=12, L3=24 |
| `boll_lower_bfq` | `double precision` | yes |  | BOLL指标，布林带-CLOSE, N=20, P=2 |
| `boll_lower_hfq` | `double precision` | yes |  | BOLL指标，布林带-CLOSE, N=20, P=2 |
| `boll_lower_qfq` | `double precision` | yes |  | BOLL指标，布林带-CLOSE, N=20, P=2 |
| `boll_mid_bfq` | `double precision` | yes |  | BOLL指标，布林带-CLOSE, N=20, P=2 |
| `boll_mid_hfq` | `double precision` | yes |  | BOLL指标，布林带-CLOSE, N=20, P=2 |
| `boll_mid_qfq` | `double precision` | yes |  | BOLL指标，布林带-CLOSE, N=20, P=2 |
| `boll_upper_bfq` | `double precision` | yes |  | BOLL指标，布林带-CLOSE, N=20, P=2 |
| `boll_upper_hfq` | `double precision` | yes |  | BOLL指标，布林带-CLOSE, N=20, P=2 |
| `boll_upper_qfq` | `double precision` | yes |  | BOLL指标，布林带-CLOSE, N=20, P=2 |
| `brar_ar_bfq` | `double precision` | yes |  | BRAR情绪指标-OPEN, CLOSE, HIGH, LOW, M1=26 |
| `brar_ar_hfq` | `double precision` | yes |  | BRAR情绪指标-OPEN, CLOSE, HIGH, LOW, M1=26 |
| `brar_ar_qfq` | `double precision` | yes |  | BRAR情绪指标-OPEN, CLOSE, HIGH, LOW, M1=26 |
| `brar_br_bfq` | `double precision` | yes |  | BRAR情绪指标-OPEN, CLOSE, HIGH, LOW, M1=26 |
| `brar_br_hfq` | `double precision` | yes |  | BRAR情绪指标-OPEN, CLOSE, HIGH, LOW, M1=26 |
| `brar_br_qfq` | `double precision` | yes |  | BRAR情绪指标-OPEN, CLOSE, HIGH, LOW, M1=26 |
| `cci_bfq` | `double precision` | yes |  | 顺势指标又叫CCI指标-CLOSE, HIGH, LOW, N=14 |
| `cci_hfq` | `double precision` | yes |  | 顺势指标又叫CCI指标-CLOSE, HIGH, LOW, N=14 |
| `cci_qfq` | `double precision` | yes |  | 顺势指标又叫CCI指标-CLOSE, HIGH, LOW, N=14 |
| `cr_bfq` | `double precision` | yes |  | CR价格动量指标-CLOSE, HIGH, LOW, N=20 |
| `cr_hfq` | `double precision` | yes |  | CR价格动量指标-CLOSE, HIGH, LOW, N=20 |
| `cr_qfq` | `double precision` | yes |  | CR价格动量指标-CLOSE, HIGH, LOW, N=20 |
| `dfma_dif_bfq` | `double precision` | yes |  | 平行线差指标-CLOSE, N1=10, N2=50, M=10 |
| `dfma_dif_hfq` | `double precision` | yes |  | 平行线差指标-CLOSE, N1=10, N2=50, M=10 |
| `dfma_dif_qfq` | `double precision` | yes |  | 平行线差指标-CLOSE, N1=10, N2=50, M=10 |
| `dfma_difma_bfq` | `double precision` | yes |  | 平行线差指标-CLOSE, N1=10, N2=50, M=10 |
| `dfma_difma_hfq` | `double precision` | yes |  | 平行线差指标-CLOSE, N1=10, N2=50, M=10 |
| `dfma_difma_qfq` | `double precision` | yes |  | 平行线差指标-CLOSE, N1=10, N2=50, M=10 |
| `dmi_adx_bfq` | `double precision` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `dmi_adx_hfq` | `double precision` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `dmi_adx_qfq` | `double precision` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `dmi_adxr_bfq` | `double precision` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `dmi_adxr_hfq` | `double precision` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `dmi_adxr_qfq` | `double precision` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `dmi_mdi_bfq` | `double precision` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `dmi_mdi_hfq` | `double precision` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `dmi_mdi_qfq` | `double precision` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `dmi_pdi_bfq` | `double precision` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `dmi_pdi_hfq` | `double precision` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `dmi_pdi_qfq` | `double precision` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `downdays` | `double precision` | yes |  | 连跌天数 |
| `updays` | `double precision` | yes |  | 连涨天数 |
| `dpo_bfq` | `double precision` | yes |  | 区间震荡线-CLOSE, M1=20, M2=10, M3=6 |
| `dpo_hfq` | `double precision` | yes |  | 区间震荡线-CLOSE, M1=20, M2=10, M3=6 |
| `dpo_qfq` | `double precision` | yes |  | 区间震荡线-CLOSE, M1=20, M2=10, M3=6 |
| `madpo_bfq` | `double precision` | yes |  | 区间震荡线-CLOSE, M1=20, M2=10, M3=6 |
| `madpo_hfq` | `double precision` | yes |  | 区间震荡线-CLOSE, M1=20, M2=10, M3=6 |
| `madpo_qfq` | `double precision` | yes |  | 区间震荡线-CLOSE, M1=20, M2=10, M3=6 |
| `ema_bfq_10` | `double precision` | yes |  | 指数移动平均-N=10 |
| `ema_bfq_20` | `double precision` | yes |  | 指数移动平均-N=20 |
| `ema_bfq_250` | `double precision` | yes |  | 指数移动平均-N=250 |
| `ema_bfq_30` | `double precision` | yes |  | 指数移动平均-N=30 |
| `ema_bfq_5` | `double precision` | yes |  | 指数移动平均-N=5 |
| `ema_bfq_60` | `double precision` | yes |  | 指数移动平均-N=60 |
| `ema_bfq_90` | `double precision` | yes |  | 指数移动平均-N=90 |
| `ema_hfq_10` | `double precision` | yes |  | 指数移动平均-N=10 |
| `ema_hfq_20` | `double precision` | yes |  | 指数移动平均-N=20 |
| `ema_hfq_250` | `double precision` | yes |  | 指数移动平均-N=250 |
| `ema_hfq_30` | `double precision` | yes |  | 指数移动平均-N=30 |
| `ema_hfq_5` | `double precision` | yes |  | 指数移动平均-N=5 |
| `ema_hfq_60` | `double precision` | yes |  | 指数移动平均-N=60 |
| `ema_hfq_90` | `double precision` | yes |  | 指数移动平均-N=90 |
| `ema_qfq_10` | `double precision` | yes |  | 指数移动平均-N=10 |
| `ema_qfq_20` | `double precision` | yes |  | 指数移动平均-N=20 |
| `ema_qfq_250` | `double precision` | yes |  | 指数移动平均-N=250 |
| `ema_qfq_30` | `double precision` | yes |  | 指数移动平均-N=30 |
| `ema_qfq_5` | `double precision` | yes |  | 指数移动平均-N=5 |
| `ema_qfq_60` | `double precision` | yes |  | 指数移动平均-N=60 |
| `ema_qfq_90` | `double precision` | yes |  | 指数移动平均-N=90 |
| `emv_bfq` | `double precision` | yes |  | 简易波动指标-HIGH, LOW, VOL, N=14, M=9 |
| `emv_hfq` | `double precision` | yes |  | 简易波动指标-HIGH, LOW, VOL, N=14, M=9 |
| `emv_qfq` | `double precision` | yes |  | 简易波动指标-HIGH, LOW, VOL, N=14, M=9 |
| `maemv_bfq` | `double precision` | yes |  | 简易波动指标-HIGH, LOW, VOL, N=14, M=9 |
| `maemv_hfq` | `double precision` | yes |  | 简易波动指标-HIGH, LOW, VOL, N=14, M=9 |
| `maemv_qfq` | `double precision` | yes |  | 简易波动指标-HIGH, LOW, VOL, N=14, M=9 |
| `expma_12_bfq` | `double precision` | yes |  | EMA指数平均数指标-CLOSE, N1=12, N2=50 |
| `expma_12_hfq` | `double precision` | yes |  | EMA指数平均数指标-CLOSE, N1=12, N2=50 |
| `expma_12_qfq` | `double precision` | yes |  | EMA指数平均数指标-CLOSE, N1=12, N2=50 |
| `expma_50_bfq` | `double precision` | yes |  | EMA指数平均数指标-CLOSE, N1=12, N2=50 |
| `expma_50_hfq` | `double precision` | yes |  | EMA指数平均数指标-CLOSE, N1=12, N2=50 |
| `expma_50_qfq` | `double precision` | yes |  | EMA指数平均数指标-CLOSE, N1=12, N2=50 |
| `kdj_bfq` | `double precision` | yes |  | KDJ指标-CLOSE, HIGH, LOW, N=9, M1=3, M2=3 |
| `kdj_hfq` | `double precision` | yes |  | KDJ指标-CLOSE, HIGH, LOW, N=9, M1=3, M2=3 |
| `kdj_qfq` | `double precision` | yes |  | KDJ指标-CLOSE, HIGH, LOW, N=9, M1=3, M2=3 |
| `kdj_d_bfq` | `double precision` | yes |  | KDJ指标-CLOSE, HIGH, LOW, N=9, M1=3, M2=3 |
| `kdj_d_hfq` | `double precision` | yes |  | KDJ指标-CLOSE, HIGH, LOW, N=9, M1=3, M2=3 |
| `kdj_d_qfq` | `double precision` | yes |  | KDJ指标-CLOSE, HIGH, LOW, N=9, M1=3, M2=3 |
| `kdj_k_bfq` | `double precision` | yes |  | KDJ指标-CLOSE, HIGH, LOW, N=9, M1=3, M2=3 |
| `kdj_k_hfq` | `double precision` | yes |  | KDJ指标-CLOSE, HIGH, LOW, N=9, M1=3, M2=3 |
| `kdj_k_qfq` | `double precision` | yes |  | KDJ指标-CLOSE, HIGH, LOW, N=9, M1=3, M2=3 |
| `ktn_down_bfq` | `double precision` | yes |  | 肯特纳交易通道, N选20日，ATR选10日-CLOSE, HIGH, LOW, N=20, M=10 |
| `ktn_down_hfq` | `double precision` | yes |  | 肯特纳交易通道, N选20日，ATR选10日-CLOSE, HIGH, LOW, N=20, M=10 |
| `ktn_down_qfq` | `double precision` | yes |  | 肯特纳交易通道, N选20日，ATR选10日-CLOSE, HIGH, LOW, N=20, M=10 |
| `ktn_mid_bfq` | `double precision` | yes |  | 肯特纳交易通道, N选20日，ATR选10日-CLOSE, HIGH, LOW, N=20, M=10 |
| `ktn_mid_hfq` | `double precision` | yes |  | 肯特纳交易通道, N选20日，ATR选10日-CLOSE, HIGH, LOW, N=20, M=10 |
| `ktn_mid_qfq` | `double precision` | yes |  | 肯特纳交易通道, N选20日，ATR选10日-CLOSE, HIGH, LOW, N=20, M=10 |
| `ktn_upper_bfq` | `double precision` | yes |  | 肯特纳交易通道, N选20日，ATR选10日-CLOSE, HIGH, LOW, N=20, M=10 |
| `ktn_upper_hfq` | `double precision` | yes |  | 肯特纳交易通道, N选20日，ATR选10日-CLOSE, HIGH, LOW, N=20, M=10 |
| `ktn_upper_qfq` | `double precision` | yes |  | 肯特纳交易通道, N选20日，ATR选10日-CLOSE, HIGH, LOW, N=20, M=10 |
| `lowdays` | `double precision` | yes |  | LOWRANGE(LOW)表示当前最低价是近多少周期内最低价的最小值 |
| `topdays` | `double precision` | yes |  | TOPRANGE(HIGH)表示当前最高价是近多少周期内最高价的最大值 |
| `ma_bfq_10` | `double precision` | yes |  | 简单移动平均-N=10 |
| `ma_bfq_20` | `double precision` | yes |  | 简单移动平均-N=20 |
| `ma_bfq_250` | `double precision` | yes |  | 简单移动平均-N=250 |
| `ma_bfq_30` | `double precision` | yes |  | 简单移动平均-N=30 |
| `ma_bfq_5` | `double precision` | yes |  | 简单移动平均-N=5 |
| `ma_bfq_60` | `double precision` | yes |  | 简单移动平均-N=60 |
| `ma_bfq_90` | `double precision` | yes |  | 简单移动平均-N=90 |
| `ma_hfq_10` | `double precision` | yes |  | 简单移动平均-N=10 |
| `ma_hfq_20` | `double precision` | yes |  | 简单移动平均-N=20 |
| `ma_hfq_250` | `double precision` | yes |  | 简单移动平均-N=250 |
| `ma_hfq_30` | `double precision` | yes |  | 简单移动平均-N=30 |
| `ma_hfq_5` | `double precision` | yes |  | 简单移动平均-N=5 |
| `ma_hfq_60` | `double precision` | yes |  | 简单移动平均-N=60 |
| `ma_hfq_90` | `double precision` | yes |  | 简单移动平均-N=90 |
| `ma_qfq_10` | `double precision` | yes |  | 简单移动平均-N=10 |
| `ma_qfq_20` | `double precision` | yes |  | 简单移动平均-N=20 |
| `ma_qfq_250` | `double precision` | yes |  | 简单移动平均-N=250 |
| `ma_qfq_30` | `double precision` | yes |  | 简单移动平均-N=30 |
| `ma_qfq_5` | `double precision` | yes |  | 简单移动平均-N=5 |
| `ma_qfq_60` | `double precision` | yes |  | 简单移动平均-N=60 |
| `ma_qfq_90` | `double precision` | yes |  | 简单移动平均-N=90 |
| `macd_bfq` | `double precision` | yes |  | MACD指标-CLOSE, SHORT=12, LONG=26, M=9 |
| `macd_hfq` | `double precision` | yes |  | MACD指标-CLOSE, SHORT=12, LONG=26, M=9 |
| `macd_qfq` | `double precision` | yes |  | MACD指标-CLOSE, SHORT=12, LONG=26, M=9 |
| `macd_dea_bfq` | `double precision` | yes |  | MACD指标-CLOSE, SHORT=12, LONG=26, M=9 |
| `macd_dea_hfq` | `double precision` | yes |  | MACD指标-CLOSE, SHORT=12, LONG=26, M=9 |
| `macd_dea_qfq` | `double precision` | yes |  | MACD指标-CLOSE, SHORT=12, LONG=26, M=9 |
| `macd_dif_bfq` | `double precision` | yes |  | MACD指标-CLOSE, SHORT=12, LONG=26, M=9 |
| `macd_dif_hfq` | `double precision` | yes |  | MACD指标-CLOSE, SHORT=12, LONG=26, M=9 |
| `macd_dif_qfq` | `double precision` | yes |  | MACD指标-CLOSE, SHORT=12, LONG=26, M=9 |
| `mass_bfq` | `double precision` | yes |  | 梅斯线-HIGH, LOW, N1=9, N2=25, M=6 |
| `mass_hfq` | `double precision` | yes |  | 梅斯线-HIGH, LOW, N1=9, N2=25, M=6 |
| `mass_qfq` | `double precision` | yes |  | 梅斯线-HIGH, LOW, N1=9, N2=25, M=6 |
| `ma_mass_bfq` | `double precision` | yes |  | 梅斯线-HIGH, LOW, N1=9, N2=25, M=6 |
| `ma_mass_hfq` | `double precision` | yes |  | 梅斯线-HIGH, LOW, N1=9, N2=25, M=6 |
| `ma_mass_qfq` | `double precision` | yes |  | 梅斯线-HIGH, LOW, N1=9, N2=25, M=6 |
| `mfi_bfq` | `double precision` | yes |  | MFI指标是成交量的RSI指标-CLOSE, HIGH, LOW, VOL, N=14 |
| `mfi_hfq` | `double precision` | yes |  | MFI指标是成交量的RSI指标-CLOSE, HIGH, LOW, VOL, N=14 |
| `mfi_qfq` | `double precision` | yes |  | MFI指标是成交量的RSI指标-CLOSE, HIGH, LOW, VOL, N=14 |
| `mtm_bfq` | `double precision` | yes |  | 动量指标-CLOSE, N=12, M=6 |
| `mtm_hfq` | `double precision` | yes |  | 动量指标-CLOSE, N=12, M=6 |
| `mtm_qfq` | `double precision` | yes |  | 动量指标-CLOSE, N=12, M=6 |
| `mtmma_bfq` | `double precision` | yes |  | 动量指标-CLOSE, N=12, M=6 |
| `mtmma_hfq` | `double precision` | yes |  | 动量指标-CLOSE, N=12, M=6 |
| `mtmma_qfq` | `double precision` | yes |  | 动量指标-CLOSE, N=12, M=6 |
| `obv_bfq` | `double precision` | yes |  | 能量潮指标-CLOSE, VOL |
| `obv_hfq` | `double precision` | yes |  | 能量潮指标-CLOSE, VOL |
| `obv_qfq` | `double precision` | yes |  | 能量潮指标-CLOSE, VOL |
| `psy_bfq` | `double precision` | yes |  | 投资者对股市涨跌产生心理波动的情绪指标-CLOSE, N=12, M=6 |
| `psy_hfq` | `double precision` | yes |  | 投资者对股市涨跌产生心理波动的情绪指标-CLOSE, N=12, M=6 |
| `psy_qfq` | `double precision` | yes |  | 投资者对股市涨跌产生心理波动的情绪指标-CLOSE, N=12, M=6 |
| `psyma_bfq` | `double precision` | yes |  | 投资者对股市涨跌产生心理波动的情绪指标-CLOSE, N=12, M=6 |
| `psyma_hfq` | `double precision` | yes |  | 投资者对股市涨跌产生心理波动的情绪指标-CLOSE, N=12, M=6 |
| `psyma_qfq` | `double precision` | yes |  | 投资者对股市涨跌产生心理波动的情绪指标-CLOSE, N=12, M=6 |
| `roc_bfq` | `double precision` | yes |  | 变动率指标-CLOSE, N=12, M=6 |
| `roc_hfq` | `double precision` | yes |  | 变动率指标-CLOSE, N=12, M=6 |
| `roc_qfq` | `double precision` | yes |  | 变动率指标-CLOSE, N=12, M=6 |
| `maroc_bfq` | `double precision` | yes |  | 变动率指标-CLOSE, N=12, M=6 |
| `maroc_hfq` | `double precision` | yes |  | 变动率指标-CLOSE, N=12, M=6 |
| `maroc_qfq` | `double precision` | yes |  | 变动率指标-CLOSE, N=12, M=6 |
| `rsi_bfq_12` | `double precision` | yes |  | RSI指标-CLOSE, N=12 |
| `rsi_bfq_24` | `double precision` | yes |  | RSI指标-CLOSE, N=24 |
| `rsi_bfq_6` | `double precision` | yes |  | RSI指标-CLOSE, N=6 |
| `rsi_hfq_12` | `double precision` | yes |  | RSI指标-CLOSE, N=12 |
| `rsi_hfq_24` | `double precision` | yes |  | RSI指标-CLOSE, N=24 |
| `rsi_hfq_6` | `double precision` | yes |  | RSI指标-CLOSE, N=6 |
| `rsi_qfq_12` | `double precision` | yes |  | RSI指标-CLOSE, N=12 |
| `rsi_qfq_24` | `double precision` | yes |  | RSI指标-CLOSE, N=24 |
| `rsi_qfq_6` | `double precision` | yes |  | RSI指标-CLOSE, N=6 |
| `taq_down_bfq` | `double precision` | yes |  | 唐安奇通道(海龟)交易指标-HIGH, LOW, 20 |
| `taq_down_hfq` | `double precision` | yes |  | 唐安奇通道(海龟)交易指标-HIGH, LOW, 20 |
| `taq_down_qfq` | `double precision` | yes |  | 唐安奇通道(海龟)交易指标-HIGH, LOW, 20 |
| `taq_mid_bfq` | `double precision` | yes |  | 唐安奇通道(海龟)交易指标-HIGH, LOW, 20 |
| `taq_mid_hfq` | `double precision` | yes |  | 唐安奇通道(海龟)交易指标-HIGH, LOW, 20 |
| `taq_mid_qfq` | `double precision` | yes |  | 唐安奇通道(海龟)交易指标-HIGH, LOW, 20 |
| `taq_up_bfq` | `double precision` | yes |  | 唐安奇通道(海龟)交易指标-HIGH, LOW, 20 |
| `taq_up_hfq` | `double precision` | yes |  | 唐安奇通道(海龟)交易指标-HIGH, LOW, 20 |
| `taq_up_qfq` | `double precision` | yes |  | 唐安奇通道(海龟)交易指标-HIGH, LOW, 20 |
| `trix_bfq` | `double precision` | yes |  | 三重指数平滑平均线-CLOSE, M1=12, M2=20 |
| `trix_hfq` | `double precision` | yes |  | 三重指数平滑平均线-CLOSE, M1=12, M2=20 |
| `trix_qfq` | `double precision` | yes |  | 三重指数平滑平均线-CLOSE, M1=12, M2=20 |
| `trma_bfq` | `double precision` | yes |  | 三重指数平滑平均线-CLOSE, M1=12, M2=20 |
| `trma_hfq` | `double precision` | yes |  | 三重指数平滑平均线-CLOSE, M1=12, M2=20 |
| `trma_qfq` | `double precision` | yes |  | 三重指数平滑平均线-CLOSE, M1=12, M2=20 |
| `vr_bfq` | `double precision` | yes |  | VR容量比率-CLOSE, VOL, M1=26 |
| `vr_hfq` | `double precision` | yes |  | VR容量比率-CLOSE, VOL, M1=26 |
| `vr_qfq` | `double precision` | yes |  | VR容量比率-CLOSE, VOL, M1=26 |
| `wr_bfq` | `double precision` | yes |  | W&R 威廉指标-CLOSE, HIGH, LOW, N=10, N1=6 |
| `wr_hfq` | `double precision` | yes |  | W&R 威廉指标-CLOSE, HIGH, LOW, N=10, N1=6 |
| `wr_qfq` | `double precision` | yes |  | W&R 威廉指标-CLOSE, HIGH, LOW, N=10, N1=6 |
| `wr1_bfq` | `double precision` | yes |  | W&R 威廉指标-CLOSE, HIGH, LOW, N=10, N1=6 |
| `wr1_hfq` | `double precision` | yes |  | W&R 威廉指标-CLOSE, HIGH, LOW, N=10, N1=6 |
| `wr1_qfq` | `double precision` | yes |  | W&R 威廉指标-CLOSE, HIGH, LOW, N=10, N1=6 |
| `xsii_td1_bfq` | `double precision` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |
| `xsii_td1_hfq` | `double precision` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |
| `xsii_td1_qfq` | `double precision` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |
| `xsii_td2_bfq` | `double precision` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |
| `xsii_td2_hfq` | `double precision` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |
| `xsii_td2_qfq` | `double precision` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |
| `xsii_td3_bfq` | `double precision` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |
| `xsii_td3_hfq` | `double precision` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |
| `xsii_td3_qfq` | `double precision` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |
| `xsii_td4_bfq` | `double precision` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |
| `xsii_td4_hfq` | `double precision` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |
| `xsii_td4_qfq` | `double precision` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |

## stock_moneyflow_ths

- API: `moneyflow_ths`
- Official doc: https://tushare.pro/wctapi/documents/348.md
- Checked at: `2026-06-26`
- SDK version: `1.4.29`
- Fetch plan: `kind`=`by_trade_date`; `params`=`trade_date`
- Write strategy: `type`=`upsert`; `key`=`trade_date`, `ts_code`
- Note: Fields are frozen from official Tushare output docs and SDK samples checked on 2026-06-26; optional output fields are included and should be requested explicitly with the Tushare fields parameter. pg_type is chosen explicitly from official docs plus source semantics, not inferred mechanically from pandas dtype.

| Field | PostgreSQL type | Nullable | Source format | Description |
| --- | --- | --- | --- | --- |
| `trade_date` | `date` | no | YYYYMMDD | 交易日期 |
| `ts_code` | `text` | no |  | 股票代码 |
| `name` | `text` | yes |  | 股票名称 |
| `pct_change` | `double precision` | yes |  | 涨跌幅 |
| `latest` | `double precision` | yes |  | 最新价 |
| `net_amount` | `double precision` | yes |  | 资金净流入(万元) |
| `net_d5_amount` | `double precision` | yes |  | 5日主力净额(万元) |
| `buy_lg_amount` | `double precision` | yes |  | 今日大单净流入额(万元) |
| `buy_lg_amount_rate` | `double precision` | yes |  | 今日大单净流入占比(%) |
| `buy_md_amount` | `double precision` | yes |  | 今日中单净流入额(万元) |
| `buy_md_amount_rate` | `double precision` | yes |  | 今日中单净流入占比(%) |
| `buy_sm_amount` | `double precision` | yes |  | 今日小单净流入额(万元) |
| `buy_sm_amount_rate` | `double precision` | yes |  | 今日小单净流入占比(%) |

## index_idx_factor_pro

- API: `idx_factor_pro`
- Official doc: https://tushare.pro/wctapi/documents/358.md
- Checked at: `2026-06-26`
- SDK version: `1.4.29`
- Fetch plan: `kind`=`by_trade_date_and_code_list`; `params`=`ts_code`, `start_date`, `end_date`; `code_source`=`INDEX_LIST_PATH enabled rows`
- Write strategy: `type`=`upsert`; `key`=`trade_date`, `ts_code`
- Note: Fields are frozen from official Tushare output docs and SDK samples checked on 2026-06-26; optional output fields are included and should be requested explicitly with the Tushare fields parameter. pg_type is chosen explicitly from official docs plus source semantics, not inferred mechanically from pandas dtype.
- Note: SDK sample used 000001.SH over 20260529-20260626.

| Field | PostgreSQL type | Nullable | Source format | Description |
| --- | --- | --- | --- | --- |
| `ts_code` | `text` | no |  | 指数代码 |
| `trade_date` | `date` | no | YYYYMMDD | 交易日期 |
| `open` | `double precision` | yes |  | 开盘价 |
| `high` | `double precision` | yes |  | 最高价 |
| `low` | `double precision` | yes |  | 最低价 |
| `close` | `double precision` | yes |  | 收盘价 |
| `pre_close` | `double precision` | yes |  | 昨收价 |
| `change` | `double precision` | yes |  | 涨跌额 |
| `pct_change` | `double precision` | yes |  | 涨跌幅 （未复权，如果是复权请用 通用行情接口 ） |
| `vol` | `double precision` | yes |  | 成交量 （手） |
| `amount` | `double precision` | yes |  | 成交额 （千元） |
| `asi_bfq` | `double precision` | yes |  | 振动升降指标-OPEN, CLOSE, HIGH, LOW, M1=26, M2=10 |
| `asit_bfq` | `double precision` | yes |  | 振动升降指标-OPEN, CLOSE, HIGH, LOW, M1=26, M2=10 |
| `atr_bfq` | `double precision` | yes |  | 真实波动N日平均值-CLOSE, HIGH, LOW, N=20 |
| `bbi_bfq` | `double precision` | yes |  | BBI多空指标-CLOSE, M1=3, M2=6, M3=12, M4=20 |
| `bias1_bfq` | `double precision` | yes |  | BIAS乖离率-CLOSE, L1=6, L2=12, L3=24 |
| `bias2_bfq` | `double precision` | yes |  | BIAS乖离率-CLOSE, L1=6, L2=12, L3=24 |
| `bias3_bfq` | `double precision` | yes |  | BIAS乖离率-CLOSE, L1=6, L2=12, L3=24 |
| `boll_lower_bfq` | `double precision` | yes |  | BOLL指标，布林带-CLOSE, N=20, P=2 |
| `boll_mid_bfq` | `double precision` | yes |  | BOLL指标，布林带-CLOSE, N=20, P=2 |
| `boll_upper_bfq` | `double precision` | yes |  | BOLL指标，布林带-CLOSE, N=20, P=2 |
| `brar_ar_bfq` | `double precision` | yes |  | BRAR情绪指标-OPEN, CLOSE, HIGH, LOW, M1=26 |
| `brar_br_bfq` | `double precision` | yes |  | BRAR情绪指标-OPEN, CLOSE, HIGH, LOW, M1=26 |
| `cci_bfq` | `double precision` | yes |  | 顺势指标又叫CCI指标-CLOSE, HIGH, LOW, N=14 |
| `cr_bfq` | `double precision` | yes |  | CR价格动量指标-CLOSE, HIGH, LOW, N=20 |
| `dfma_dif_bfq` | `double precision` | yes |  | 平行线差指标-CLOSE, N1=10, N2=50, M=10 |
| `dfma_difma_bfq` | `double precision` | yes |  | 平行线差指标-CLOSE, N1=10, N2=50, M=10 |
| `dmi_adx_bfq` | `double precision` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `dmi_adxr_bfq` | `double precision` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `dmi_mdi_bfq` | `double precision` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `dmi_pdi_bfq` | `double precision` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `downdays` | `double precision` | yes |  | 连跌天数 |
| `updays` | `double precision` | yes |  | 连涨天数 |
| `dpo_bfq` | `double precision` | yes |  | 区间震荡线-CLOSE, M1=20, M2=10, M3=6 |
| `madpo_bfq` | `double precision` | yes |  | 区间震荡线-CLOSE, M1=20, M2=10, M3=6 |
| `ema_bfq_10` | `double precision` | yes |  | 指数移动平均-N=10 |
| `ema_bfq_20` | `double precision` | yes |  | 指数移动平均-N=20 |
| `ema_bfq_250` | `double precision` | yes |  | 指数移动平均-N=250 |
| `ema_bfq_30` | `double precision` | yes |  | 指数移动平均-N=30 |
| `ema_bfq_5` | `double precision` | yes |  | 指数移动平均-N=5 |
| `ema_bfq_60` | `double precision` | yes |  | 指数移动平均-N=60 |
| `ema_bfq_90` | `double precision` | yes |  | 指数移动平均-N=90 |
| `emv_bfq` | `double precision` | yes |  | 简易波动指标-HIGH, LOW, VOL, N=14, M=9 |
| `maemv_bfq` | `double precision` | yes |  | 简易波动指标-HIGH, LOW, VOL, N=14, M=9 |
| `expma_12_bfq` | `double precision` | yes |  | EMA指数平均数指标-CLOSE, N1=12, N2=50 |
| `expma_50_bfq` | `double precision` | yes |  | EMA指数平均数指标-CLOSE, N1=12, N2=50 |
| `kdj_bfq` | `double precision` | yes |  | KDJ指标-CLOSE, HIGH, LOW, N=9, M1=3, M2=3 |
| `kdj_d_bfq` | `double precision` | yes |  | KDJ指标-CLOSE, HIGH, LOW, N=9, M1=3, M2=3 |
| `kdj_k_bfq` | `double precision` | yes |  | KDJ指标-CLOSE, HIGH, LOW, N=9, M1=3, M2=3 |
| `ktn_down_bfq` | `double precision` | yes |  | 肯特纳交易通道, N选20日，ATR选10日-CLOSE, HIGH, LOW, N=20, M=10 |
| `ktn_mid_bfq` | `double precision` | yes |  | 肯特纳交易通道, N选20日，ATR选10日-CLOSE, HIGH, LOW, N=20, M=10 |
| `ktn_upper_bfq` | `double precision` | yes |  | 肯特纳交易通道, N选20日，ATR选10日-CLOSE, HIGH, LOW, N=20, M=10 |
| `lowdays` | `double precision` | yes |  | LOWRANGE(LOW)表示当前最低价是近多少周期内最低价的最小值 |
| `topdays` | `double precision` | yes |  | TOPRANGE(HIGH)表示当前最高价是近多少周期内最高价的最大值 |
| `ma_bfq_10` | `double precision` | yes |  | 简单移动平均-N=10 |
| `ma_bfq_20` | `double precision` | yes |  | 简单移动平均-N=20 |
| `ma_bfq_250` | `double precision` | yes |  | 简单移动平均-N=250 |
| `ma_bfq_30` | `double precision` | yes |  | 简单移动平均-N=30 |
| `ma_bfq_5` | `double precision` | yes |  | 简单移动平均-N=5 |
| `ma_bfq_60` | `double precision` | yes |  | 简单移动平均-N=60 |
| `ma_bfq_90` | `double precision` | yes |  | 简单移动平均-N=90 |
| `macd_bfq` | `double precision` | yes |  | MACD指标-CLOSE, SHORT=12, LONG=26, M=9 |
| `macd_dea_bfq` | `double precision` | yes |  | MACD指标-CLOSE, SHORT=12, LONG=26, M=9 |
| `macd_dif_bfq` | `double precision` | yes |  | MACD指标-CLOSE, SHORT=12, LONG=26, M=9 |
| `mass_bfq` | `double precision` | yes |  | 梅斯线-HIGH, LOW, N1=9, N2=25, M=6 |
| `ma_mass_bfq` | `double precision` | yes |  | 梅斯线-HIGH, LOW, N1=9, N2=25, M=6 |
| `mfi_bfq` | `double precision` | yes |  | MFI指标是成交量的RSI指标-CLOSE, HIGH, LOW, VOL, N=14 |
| `mtm_bfq` | `double precision` | yes |  | 动量指标-CLOSE, N=12, M=6 |
| `mtmma_bfq` | `double precision` | yes |  | 动量指标-CLOSE, N=12, M=6 |
| `obv_bfq` | `double precision` | yes |  | 能量潮指标-CLOSE, VOL |
| `psy_bfq` | `double precision` | yes |  | 投资者对股市涨跌产生心理波动的情绪指标-CLOSE, N=12, M=6 |
| `psyma_bfq` | `double precision` | yes |  | 投资者对股市涨跌产生心理波动的情绪指标-CLOSE, N=12, M=6 |
| `roc_bfq` | `double precision` | yes |  | 变动率指标-CLOSE, N=12, M=6 |
| `maroc_bfq` | `double precision` | yes |  | 变动率指标-CLOSE, N=12, M=6 |
| `rsi_bfq_12` | `double precision` | yes |  | RSI指标-CLOSE, N=12 |
| `rsi_bfq_24` | `double precision` | yes |  | RSI指标-CLOSE, N=24 |
| `rsi_bfq_6` | `double precision` | yes |  | RSI指标-CLOSE, N=6 |
| `taq_down_bfq` | `double precision` | yes |  | 唐安奇通道(海龟)交易指标-HIGH, LOW, 20 |
| `taq_mid_bfq` | `double precision` | yes |  | 唐安奇通道(海龟)交易指标-HIGH, LOW, 20 |
| `taq_up_bfq` | `double precision` | yes |  | 唐安奇通道(海龟)交易指标-HIGH, LOW, 20 |
| `trix_bfq` | `double precision` | yes |  | 三重指数平滑平均线-CLOSE, M1=12, M2=20 |
| `trma_bfq` | `double precision` | yes |  | 三重指数平滑平均线-CLOSE, M1=12, M2=20 |
| `vr_bfq` | `double precision` | yes |  | VR容量比率-CLOSE, VOL, M1=26 |
| `wr_bfq` | `double precision` | yes |  | W&R 威廉指标-CLOSE, HIGH, LOW, N=10, N1=6 |
| `wr1_bfq` | `double precision` | yes |  | W&R 威廉指标-CLOSE, HIGH, LOW, N=10, N1=6 |
| `xsii_td1_bfq` | `double precision` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |
| `xsii_td2_bfq` | `double precision` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |
| `xsii_td3_bfq` | `double precision` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |
| `xsii_td4_bfq` | `double precision` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |

## ths_index

- API: `ths_index`
- Official doc: https://tushare.pro/wctapi/documents/259.md
- Checked at: `2026-06-26`
- SDK version: `1.4.29`
- Fetch plan: `kind`=`refresh_current`; `params`=
- Write strategy: `type`=`replace_all`; `key`=`ts_code`; `transaction_group`=`ths_classification`
- Note: Fields are frozen from official Tushare output docs and SDK samples checked on 2026-06-26; optional output fields are included and should be requested explicitly with the Tushare fields parameter. pg_type is chosen explicitly from official docs plus source semantics, not inferred mechanically from pandas dtype.
- Note: Current classification list only; no historical snapshot columns are added.

| Field | PostgreSQL type | Nullable | Source format | Description |
| --- | --- | --- | --- | --- |
| `ts_code` | `text` | no |  | 代码 |
| `name` | `text` | yes |  | 名称 |
| `count` | `integer` | yes |  | 成分个数 |
| `exchange` | `text` | yes |  | 交易所 |
| `list_date` | `date` | yes | YYYYMMDD | 上市日期 |
| `type` | `text` | yes |  | N概念指数S特色指数 |

## ths_daily

- API: `ths_daily`
- Official doc: https://tushare.pro/wctapi/documents/260.md
- Checked at: `2026-06-26`
- SDK version: `1.4.29`
- Fetch plan: `kind`=`by_trade_date_and_current_ths_index`; `params`=`ts_code`, `start_date`, `end_date`; `code_source`=`ths_index`
- Write strategy: `type`=`upsert`; `key`=`trade_date`, `ts_code`
- Note: Fields are frozen from official Tushare output docs and SDK samples checked on 2026-06-26; optional output fields are included and should be requested explicitly with the Tushare fields parameter. pg_type is chosen explicitly from official docs plus source semantics, not inferred mechanically from pandas dtype.
- Note: Optional total_mv and float_mv were confirmed with an explicit fields request in SDK 1.4.29.

| Field | PostgreSQL type | Nullable | Source format | Description |
| --- | --- | --- | --- | --- |
| `ts_code` | `text` | no |  | TS指数代码 |
| `trade_date` | `date` | no | YYYYMMDD | 交易日 |
| `open` | `double precision` | yes |  | 开盘点位 |
| `high` | `double precision` | yes |  | 最高点位 |
| `low` | `double precision` | yes |  | 最低点位 |
| `close` | `double precision` | yes |  | 收盘点位 |
| `pre_close` | `double precision` | yes |  | 昨日收盘点 |
| `avg_price` | `double precision` | yes |  | 平均价 |
| `change` | `double precision` | yes |  | 涨跌点位 |
| `pct_change` | `double precision` | yes |  | 涨跌幅 |
| `vol` | `double precision` | yes |  | 成交量（手） |
| `turnover_rate` | `double precision` | yes |  | 换手率（%） |
| `total_mv` | `double precision` | yes |  | 总市值（元） |
| `float_mv` | `double precision` | yes |  | 流通市值（元） |

## moneyflow_cnt_ths

- API: `moneyflow_cnt_ths`
- Official doc: https://tushare.pro/wctapi/documents/371.md
- Checked at: `2026-06-26`
- SDK version: `1.4.29`
- Fetch plan: `kind`=`by_trade_date`; `params`=`trade_date`
- Write strategy: `type`=`upsert`; `key`=`trade_date`, `ts_code`
- Note: Fields are frozen from official Tushare output docs and SDK samples checked on 2026-06-26; optional output fields are included and should be requested explicitly with the Tushare fields parameter. pg_type is chosen explicitly from official docs plus source semantics, not inferred mechanically from pandas dtype.

| Field | PostgreSQL type | Nullable | Source format | Description |
| --- | --- | --- | --- | --- |
| `trade_date` | `date` | no | YYYYMMDD | 交易日期 |
| `ts_code` | `text` | no |  | 板块代码 |
| `name` | `text` | yes |  | 板块名称 |
| `lead_stock` | `text` | yes |  | 领涨股票名称 |
| `close_price` | `double precision` | yes |  | 最新价 |
| `pct_change` | `double precision` | yes |  | 行业涨跌幅 |
| `industry_index` | `double precision` | yes |  | 板块指数点位 |
| `company_num` | `integer` | yes |  | 公司数量 |
| `pct_change_stock` | `double precision` | yes |  | 领涨股涨跌幅 |
| `net_buy_amount` | `double precision` | yes |  | 流入资金(亿元) |
| `net_sell_amount` | `double precision` | yes |  | 流出资金(亿元) |
| `net_amount` | `double precision` | yes |  | 净额(亿元) |

## ths_member

- API: `ths_member`
- Official doc: https://tushare.pro/wctapi/documents/261.md
- Checked at: `2026-06-26`
- SDK version: `1.4.29`
- Fetch plan: `kind`=`refresh_current_by_ths_code`; `params`=`ts_code`; `code_source`=`ths_index`
- Write strategy: `type`=`replace_all`; `key`=`ts_code`, `con_code`; `transaction_group`=`ths_classification`
- Note: Fields are frozen from official Tushare output docs and SDK samples checked on 2026-06-26; optional output fields are included and should be requested explicitly with the Tushare fields parameter. pg_type is chosen explicitly from official docs plus source semantics, not inferred mechanically from pandas dtype.
- Note: Current membership relation only; no daily membership snapshot is stored.
- Note: Optional weight, in_date, out_date, and is_new are included. SDK 1.4.29 explicit fields sample returned is_new and returned weight/in_date/out_date as all NULL for the sampled THS index.

| Field | PostgreSQL type | Nullable | Source format | Description |
| --- | --- | --- | --- | --- |
| `ts_code` | `text` | no |  | 指数代码 |
| `con_code` | `text` | no |  | 股票代码 |
| `con_name` | `text` | yes |  | 股票名称 |
| `weight` | `double precision` | yes |  | 权重(暂无) |
| `in_date` | `date` | yes | YYYYMMDD | 纳入日期(暂无) |
| `out_date` | `date` | yes | YYYYMMDD | 剔除日期(暂无) |
| `is_new` | `text` | yes |  | 是否最新Y是N否 |

## limit_cpt_list

- API: `limit_cpt_list`
- Official doc: https://tushare.pro/wctapi/documents/357.md
- Checked at: `2026-06-26`
- SDK version: `1.4.29`
- Fetch plan: `kind`=`by_trade_date`; `params`=`trade_date`
- Write strategy: `type`=`delete_insert_by_date`; `date_field`=`trade_date`; `natural_key_candidate`=`trade_date`, `rank`, `ts_code`
- Note: Fields are frozen from official Tushare output docs and SDK samples checked on 2026-06-26; optional output fields are included and should be requested explicitly with the Tushare fields parameter. pg_type is chosen explicitly from official docs plus source semantics, not inferred mechanically from pandas dtype.
- Note: No stable unique key is guaranteed by the docs; v1 uses date-slice delete then insert.
- Doc mismatch: Official docs describe cons_nums as int, but SDK 1.4.29 sample returned string values; v1 stores cons_nums as text.
- Doc mismatch: Official docs describe rank as str, but SDK 1.4.29 sample returned integer values; v1 stores rank as integer.

| Field | PostgreSQL type | Nullable | Source format | Description |
| --- | --- | --- | --- | --- |
| `ts_code` | `text` | no |  | 板块代码 |
| `name` | `text` | yes |  | 板块名称 |
| `trade_date` | `date` | no | YYYYMMDD | 交易日期 |
| `days` | `integer` | yes |  | 上榜天数 |
| `up_stat` | `text` | yes |  | 连板高度 |
| `cons_nums` | `text` | yes |  | 连板家数 |
| `up_nums` | `integer` | yes |  | 涨停家数 |
| `pct_chg` | `double precision` | yes |  | 涨跌幅% |
| `rank` | `integer` | yes |  | 板块热点排名 |

## limit_list_ths

- API: `limit_list_ths`
- Official doc: https://tushare.pro/wctapi/documents/355.md
- Checked at: `2026-06-26`
- SDK version: `1.4.29`
- Fetch plan: `kind`=`by_trade_date`; `params`=`trade_date`
- Write strategy: `type`=`upsert`; `key`=`trade_date`, `ts_code`
- Note: Fields are frozen from official Tushare output docs and SDK samples checked on 2026-06-26; optional output fields are included and should be requested explicitly with the Tushare fields parameter. pg_type is chosen explicitly from official docs plus source semantics, not inferred mechanically from pandas dtype.
- Note: Optional first_lu_time, last_lu_time, first_ld_time, last_ld_time, rise_rate, and sum_float are included. SDK 1.4.29 explicit fields sample returned first_ld_time, last_ld_time, lu_limit_order, and sum_float as all NULL for the sampled date.

| Field | PostgreSQL type | Nullable | Source format | Description |
| --- | --- | --- | --- | --- |
| `trade_date` | `date` | no | YYYYMMDD | 交易日期 |
| `ts_code` | `text` | no |  | 股票代码 |
| `name` | `text` | yes |  | 股票名称 |
| `price` | `double precision` | yes |  | 收盘价(元) |
| `pct_chg` | `double precision` | yes |  | 涨跌幅% |
| `open_num` | `integer` | yes |  | 打开次数 |
| `lu_desc` | `text` | yes |  | 涨停原因 |
| `limit_type` | `text` | yes |  | 板单类别 |
| `tag` | `text` | yes |  | 涨停标签 |
| `status` | `text` | yes |  | 涨停状态（N连板、一字板） |
| `first_lu_time` | `timestamp without time zone` | yes | YYYY-MM-DD HH:MM:SS | 首次涨停时间 |
| `last_lu_time` | `timestamp without time zone` | yes | YYYY-MM-DD HH:MM:SS | 最后涨停时间 |
| `first_ld_time` | `timestamp without time zone` | yes | YYYY-MM-DD HH:MM:SS | 首次跌停时间 |
| `last_ld_time` | `timestamp without time zone` | yes | YYYY-MM-DD HH:MM:SS | 最后跌停时间 |
| `limit_order` | `double precision` | yes |  | 封单量(元或个) |
| `limit_amount` | `double precision` | yes |  | 封单额(元或个) |
| `turnover_rate` | `double precision` | yes |  | 换手率% |
| `free_float` | `double precision` | yes |  | 实际流通(元或个) |
| `lu_limit_order` | `double precision` | yes |  | 最大封单(元或个) |
| `limit_up_suc_rate` | `double precision` | yes |  | 近一年涨停封板率 |
| `turnover` | `double precision` | yes |  | 成交额 |
| `rise_rate` | `double precision` | yes |  | 涨速 |
| `sum_float` | `double precision` | yes |  | 总市值（亿元） |
| `market_type` | `text` | yes |  | 股票类型：HS沪深主板、GEM创业板、STAR科创板 |
