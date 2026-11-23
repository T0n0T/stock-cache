# Tushare Fields

This file is generated from `schemas/tushare-fields.yaml`.
Do not edit it by hand; update the YAML and rerun `tools/generate_tushare_fields_doc.py`.

## stock_basic

- API: `stock_basic`
- Official doc: https://tushare.pro/document/2?doc_id=25
- Checked at: `2026-06-30`
- SDK version: `1.4.29`
- Fetch plan: `kind`=`refresh_current`; `params`=
- Write strategy: `type`=`replace_all`; `key`=`ts_code`; `transaction_group`=`reference_snapshot`
- Note: Current A-share stock list snapshot used to identify available stocks and their names. Refresh explicitly with the generic refresh stage together with THS classification tables.
- Note: Fields are frozen from official Tushare output docs checked on 2026-06-30; optional output fields are included and should be requested explicitly with the Tushare fields parameter.

| Field | DuckDB type | Nullable | Source format | Description |
| --- | --- | --- | --- | --- |
| `ts_code` | `VARCHAR` | no |  | TS代码 |
| `symbol` | `VARCHAR` | yes |  | 股票代码 |
| `name` | `VARCHAR` | yes |  | 股票名称 |
| `area` | `VARCHAR` | yes |  | 地域 |
| `industry` | `VARCHAR` | yes |  | 所属行业 |
| `fullname` | `VARCHAR` | yes |  | 股票全称 |
| `enname` | `VARCHAR` | yes |  | 英文全称 |
| `cnspell` | `VARCHAR` | yes |  | 拼音缩写 |
| `market` | `VARCHAR` | yes |  | 市场类型 |
| `exchange` | `VARCHAR` | yes |  | 交易所代码 |
| `curr_type` | `VARCHAR` | yes |  | 交易货币 |
| `list_status` | `VARCHAR` | yes |  | 上市状态 |
| `list_date` | `DATE` | yes | YYYYMMDD | 上市日期 |
| `delist_date` | `DATE` | yes | YYYYMMDD | 退市日期 |
| `is_hs` | `VARCHAR` | yes |  | 是否沪深港通标的 |
| `act_name` | `VARCHAR` | yes |  | 实控人名称 |
| `act_ent_type` | `VARCHAR` | yes |  | 实控人企业性质 |

## stock_stk_factor_pro

- API: `stk_factor_pro`
- Official doc: https://tushare.pro/wctapi/documents/328.md
- Checked at: `2026-06-26`
- SDK version: `1.4.29`
- Fetch plan: `kind`=`by_trade_date`; `params`=`trade_date`
- Write strategy: `type`=`upsert`; `key`=`trade_date`, `ts_code`
- Note: Fields are frozen from official Tushare output docs and SDK samples checked on 2026-06-26; optional output fields are included and should be requested explicitly with the Tushare fields parameter. DuckDB types are chosen explicitly from official docs plus source semantics, not inferred mechanically from pandas dtype.
- Note: Official document id 328 was checked; SDK sample used 000001.SZ over 20260529-20260626.

| Field | DuckDB type | Nullable | Source format | Description |
| --- | --- | --- | --- | --- |
| `ts_code` | `VARCHAR` | no |  | 股票代码 |
| `trade_date` | `DATE` | no | YYYYMMDD | 交易日期 |
| `open` | `DOUBLE` | yes |  | 开盘价 |
| `open_hfq` | `DOUBLE` | yes |  | 开盘价（后复权） |
| `open_qfq` | `DOUBLE` | yes |  | 开盘价（前复权） |
| `high` | `DOUBLE` | yes |  | 最高价 |
| `high_hfq` | `DOUBLE` | yes |  | 最高价（后复权） |
| `high_qfq` | `DOUBLE` | yes |  | 最高价（前复权） |
| `low` | `DOUBLE` | yes |  | 最低价 |
| `low_hfq` | `DOUBLE` | yes |  | 最低价（后复权） |
| `low_qfq` | `DOUBLE` | yes |  | 最低价（前复权） |
| `close` | `DOUBLE` | yes |  | 收盘价 |
| `close_hfq` | `DOUBLE` | yes |  | 收盘价（后复权） |
| `close_qfq` | `DOUBLE` | yes |  | 收盘价（前复权） |
| `pre_close` | `DOUBLE` | yes |  | 昨收价(前复权)--为daily接口的pre_close,以当时复权因子计算值跟前一日close_qfq对不上，可不用 |
| `change` | `DOUBLE` | yes |  | 涨跌额 |
| `pct_chg` | `DOUBLE` | yes |  | 涨跌幅 （除权后的涨跌幅） |
| `vol` | `DOUBLE` | yes |  | 成交量 （手） |
| `amount` | `DOUBLE` | yes |  | 成交额 （千元） |
| `turnover_rate` | `DOUBLE` | yes |  | 换手率（%） |
| `turnover_rate_f` | `DOUBLE` | yes |  | 换手率（自由流通股） |
| `volume_ratio` | `DOUBLE` | yes |  | 量比 |
| `pe` | `DOUBLE` | yes |  | 市盈率（总市值/净利润， 亏损的PE为空） |
| `pe_ttm` | `DOUBLE` | yes |  | 市盈率（TTM，亏损的PE为空） |
| `pb` | `DOUBLE` | yes |  | 市净率（总市值/净资产） |
| `ps` | `DOUBLE` | yes |  | 市销率 |
| `ps_ttm` | `DOUBLE` | yes |  | 市销率（TTM） |
| `dv_ratio` | `DOUBLE` | yes |  | 股息率 （%） |
| `dv_ttm` | `DOUBLE` | yes |  | 股息率（TTM）（%） |
| `total_share` | `DOUBLE` | yes |  | 总股本 （万股） |
| `float_share` | `DOUBLE` | yes |  | 流通股本 （万股） |
| `free_share` | `DOUBLE` | yes |  | 自由流通股本 （万） |
| `total_mv` | `DOUBLE` | yes |  | 总市值 （万元） |
| `circ_mv` | `DOUBLE` | yes |  | 流通市值（万元） |
| `adj_factor` | `DOUBLE` | yes |  | 复权因子 |
| `asi_bfq` | `DOUBLE` | yes |  | 振动升降指标-OPEN, CLOSE, HIGH, LOW, M1=26, M2=10 |
| `asi_hfq` | `DOUBLE` | yes |  | 振动升降指标-OPEN, CLOSE, HIGH, LOW, M1=26, M2=10 |
| `asi_qfq` | `DOUBLE` | yes |  | 振动升降指标-OPEN, CLOSE, HIGH, LOW, M1=26, M2=10 |
| `asit_bfq` | `DOUBLE` | yes |  | 振动升降指标-OPEN, CLOSE, HIGH, LOW, M1=26, M2=10 |
| `asit_hfq` | `DOUBLE` | yes |  | 振动升降指标-OPEN, CLOSE, HIGH, LOW, M1=26, M2=10 |
| `asit_qfq` | `DOUBLE` | yes |  | 振动升降指标-OPEN, CLOSE, HIGH, LOW, M1=26, M2=10 |
| `atr_bfq` | `DOUBLE` | yes |  | 真实波动N日平均值-CLOSE, HIGH, LOW, N=20 |
| `atr_hfq` | `DOUBLE` | yes |  | 真实波动N日平均值-CLOSE, HIGH, LOW, N=20 |
| `atr_qfq` | `DOUBLE` | yes |  | 真实波动N日平均值-CLOSE, HIGH, LOW, N=20 |
| `bbi_bfq` | `DOUBLE` | yes |  | BBI多空指标-CLOSE, M1=3, M2=6, M3=12, M4=20 |
| `bbi_hfq` | `DOUBLE` | yes |  | BBI多空指标-CLOSE, M1=3, M2=6, M3=12, M4=21 |
| `bbi_qfq` | `DOUBLE` | yes |  | BBI多空指标-CLOSE, M1=3, M2=6, M3=12, M4=22 |
| `bias1_bfq` | `DOUBLE` | yes |  | BIAS乖离率-CLOSE, L1=6, L2=12, L3=24 |
| `bias1_hfq` | `DOUBLE` | yes |  | BIAS乖离率-CLOSE, L1=6, L2=12, L3=24 |
| `bias1_qfq` | `DOUBLE` | yes |  | BIAS乖离率-CLOSE, L1=6, L2=12, L3=24 |
| `bias2_bfq` | `DOUBLE` | yes |  | BIAS乖离率-CLOSE, L1=6, L2=12, L3=24 |
| `bias2_hfq` | `DOUBLE` | yes |  | BIAS乖离率-CLOSE, L1=6, L2=12, L3=24 |
| `bias2_qfq` | `DOUBLE` | yes |  | BIAS乖离率-CLOSE, L1=6, L2=12, L3=24 |
| `bias3_bfq` | `DOUBLE` | yes |  | BIAS乖离率-CLOSE, L1=6, L2=12, L3=24 |
| `bias3_hfq` | `DOUBLE` | yes |  | BIAS乖离率-CLOSE, L1=6, L2=12, L3=24 |
| `bias3_qfq` | `DOUBLE` | yes |  | BIAS乖离率-CLOSE, L1=6, L2=12, L3=24 |
| `boll_lower_bfq` | `DOUBLE` | yes |  | BOLL指标，布林带-CLOSE, N=20, P=2 |
| `boll_lower_hfq` | `DOUBLE` | yes |  | BOLL指标，布林带-CLOSE, N=20, P=2 |
| `boll_lower_qfq` | `DOUBLE` | yes |  | BOLL指标，布林带-CLOSE, N=20, P=2 |
| `boll_mid_bfq` | `DOUBLE` | yes |  | BOLL指标，布林带-CLOSE, N=20, P=2 |
| `boll_mid_hfq` | `DOUBLE` | yes |  | BOLL指标，布林带-CLOSE, N=20, P=2 |
| `boll_mid_qfq` | `DOUBLE` | yes |  | BOLL指标，布林带-CLOSE, N=20, P=2 |
| `boll_upper_bfq` | `DOUBLE` | yes |  | BOLL指标，布林带-CLOSE, N=20, P=2 |
| `boll_upper_hfq` | `DOUBLE` | yes |  | BOLL指标，布林带-CLOSE, N=20, P=2 |
| `boll_upper_qfq` | `DOUBLE` | yes |  | BOLL指标，布林带-CLOSE, N=20, P=2 |
| `brar_ar_bfq` | `DOUBLE` | yes |  | BRAR情绪指标-OPEN, CLOSE, HIGH, LOW, M1=26 |
| `brar_ar_hfq` | `DOUBLE` | yes |  | BRAR情绪指标-OPEN, CLOSE, HIGH, LOW, M1=26 |
| `brar_ar_qfq` | `DOUBLE` | yes |  | BRAR情绪指标-OPEN, CLOSE, HIGH, LOW, M1=26 |
| `brar_br_bfq` | `DOUBLE` | yes |  | BRAR情绪指标-OPEN, CLOSE, HIGH, LOW, M1=26 |
| `brar_br_hfq` | `DOUBLE` | yes |  | BRAR情绪指标-OPEN, CLOSE, HIGH, LOW, M1=26 |
| `brar_br_qfq` | `DOUBLE` | yes |  | BRAR情绪指标-OPEN, CLOSE, HIGH, LOW, M1=26 |
| `cci_bfq` | `DOUBLE` | yes |  | 顺势指标又叫CCI指标-CLOSE, HIGH, LOW, N=14 |
| `cci_hfq` | `DOUBLE` | yes |  | 顺势指标又叫CCI指标-CLOSE, HIGH, LOW, N=14 |
| `cci_qfq` | `DOUBLE` | yes |  | 顺势指标又叫CCI指标-CLOSE, HIGH, LOW, N=14 |
| `cr_bfq` | `DOUBLE` | yes |  | CR价格动量指标-CLOSE, HIGH, LOW, N=20 |
| `cr_hfq` | `DOUBLE` | yes |  | CR价格动量指标-CLOSE, HIGH, LOW, N=20 |
| `cr_qfq` | `DOUBLE` | yes |  | CR价格动量指标-CLOSE, HIGH, LOW, N=20 |
| `dfma_dif_bfq` | `DOUBLE` | yes |  | 平行线差指标-CLOSE, N1=10, N2=50, M=10 |
| `dfma_dif_hfq` | `DOUBLE` | yes |  | 平行线差指标-CLOSE, N1=10, N2=50, M=10 |
| `dfma_dif_qfq` | `DOUBLE` | yes |  | 平行线差指标-CLOSE, N1=10, N2=50, M=10 |
| `dfma_difma_bfq` | `DOUBLE` | yes |  | 平行线差指标-CLOSE, N1=10, N2=50, M=10 |
| `dfma_difma_hfq` | `DOUBLE` | yes |  | 平行线差指标-CLOSE, N1=10, N2=50, M=10 |
| `dfma_difma_qfq` | `DOUBLE` | yes |  | 平行线差指标-CLOSE, N1=10, N2=50, M=10 |
| `dmi_adx_bfq` | `DOUBLE` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `dmi_adx_hfq` | `DOUBLE` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `dmi_adx_qfq` | `DOUBLE` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `dmi_adxr_bfq` | `DOUBLE` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `dmi_adxr_hfq` | `DOUBLE` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `dmi_adxr_qfq` | `DOUBLE` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `dmi_mdi_bfq` | `DOUBLE` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `dmi_mdi_hfq` | `DOUBLE` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `dmi_mdi_qfq` | `DOUBLE` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `dmi_pdi_bfq` | `DOUBLE` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `dmi_pdi_hfq` | `DOUBLE` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `dmi_pdi_qfq` | `DOUBLE` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `downdays` | `DOUBLE` | yes |  | 连跌天数 |
| `updays` | `DOUBLE` | yes |  | 连涨天数 |
| `dpo_bfq` | `DOUBLE` | yes |  | 区间震荡线-CLOSE, M1=20, M2=10, M3=6 |
| `dpo_hfq` | `DOUBLE` | yes |  | 区间震荡线-CLOSE, M1=20, M2=10, M3=6 |
| `dpo_qfq` | `DOUBLE` | yes |  | 区间震荡线-CLOSE, M1=20, M2=10, M3=6 |
| `madpo_bfq` | `DOUBLE` | yes |  | 区间震荡线-CLOSE, M1=20, M2=10, M3=6 |
| `madpo_hfq` | `DOUBLE` | yes |  | 区间震荡线-CLOSE, M1=20, M2=10, M3=6 |
| `madpo_qfq` | `DOUBLE` | yes |  | 区间震荡线-CLOSE, M1=20, M2=10, M3=6 |
| `ema_bfq_10` | `DOUBLE` | yes |  | 指数移动平均-N=10 |
| `ema_bfq_20` | `DOUBLE` | yes |  | 指数移动平均-N=20 |
| `ema_bfq_250` | `DOUBLE` | yes |  | 指数移动平均-N=250 |
| `ema_bfq_30` | `DOUBLE` | yes |  | 指数移动平均-N=30 |
| `ema_bfq_5` | `DOUBLE` | yes |  | 指数移动平均-N=5 |
| `ema_bfq_60` | `DOUBLE` | yes |  | 指数移动平均-N=60 |
| `ema_bfq_90` | `DOUBLE` | yes |  | 指数移动平均-N=90 |
| `ema_hfq_10` | `DOUBLE` | yes |  | 指数移动平均-N=10 |
| `ema_hfq_20` | `DOUBLE` | yes |  | 指数移动平均-N=20 |
| `ema_hfq_250` | `DOUBLE` | yes |  | 指数移动平均-N=250 |
| `ema_hfq_30` | `DOUBLE` | yes |  | 指数移动平均-N=30 |
| `ema_hfq_5` | `DOUBLE` | yes |  | 指数移动平均-N=5 |
| `ema_hfq_60` | `DOUBLE` | yes |  | 指数移动平均-N=60 |
| `ema_hfq_90` | `DOUBLE` | yes |  | 指数移动平均-N=90 |
| `ema_qfq_10` | `DOUBLE` | yes |  | 指数移动平均-N=10 |
| `ema_qfq_20` | `DOUBLE` | yes |  | 指数移动平均-N=20 |
| `ema_qfq_250` | `DOUBLE` | yes |  | 指数移动平均-N=250 |
| `ema_qfq_30` | `DOUBLE` | yes |  | 指数移动平均-N=30 |
| `ema_qfq_5` | `DOUBLE` | yes |  | 指数移动平均-N=5 |
| `ema_qfq_60` | `DOUBLE` | yes |  | 指数移动平均-N=60 |
| `ema_qfq_90` | `DOUBLE` | yes |  | 指数移动平均-N=90 |
| `emv_bfq` | `DOUBLE` | yes |  | 简易波动指标-HIGH, LOW, VOL, N=14, M=9 |
| `emv_hfq` | `DOUBLE` | yes |  | 简易波动指标-HIGH, LOW, VOL, N=14, M=9 |
| `emv_qfq` | `DOUBLE` | yes |  | 简易波动指标-HIGH, LOW, VOL, N=14, M=9 |
| `maemv_bfq` | `DOUBLE` | yes |  | 简易波动指标-HIGH, LOW, VOL, N=14, M=9 |
| `maemv_hfq` | `DOUBLE` | yes |  | 简易波动指标-HIGH, LOW, VOL, N=14, M=9 |
| `maemv_qfq` | `DOUBLE` | yes |  | 简易波动指标-HIGH, LOW, VOL, N=14, M=9 |
| `expma_12_bfq` | `DOUBLE` | yes |  | EMA指数平均数指标-CLOSE, N1=12, N2=50 |
| `expma_12_hfq` | `DOUBLE` | yes |  | EMA指数平均数指标-CLOSE, N1=12, N2=50 |
| `expma_12_qfq` | `DOUBLE` | yes |  | EMA指数平均数指标-CLOSE, N1=12, N2=50 |
| `expma_50_bfq` | `DOUBLE` | yes |  | EMA指数平均数指标-CLOSE, N1=12, N2=50 |
| `expma_50_hfq` | `DOUBLE` | yes |  | EMA指数平均数指标-CLOSE, N1=12, N2=50 |
| `expma_50_qfq` | `DOUBLE` | yes |  | EMA指数平均数指标-CLOSE, N1=12, N2=50 |
| `kdj_bfq` | `DOUBLE` | yes |  | KDJ指标-CLOSE, HIGH, LOW, N=9, M1=3, M2=3 |
| `kdj_hfq` | `DOUBLE` | yes |  | KDJ指标-CLOSE, HIGH, LOW, N=9, M1=3, M2=3 |
| `kdj_qfq` | `DOUBLE` | yes |  | KDJ指标-CLOSE, HIGH, LOW, N=9, M1=3, M2=3 |
| `kdj_d_bfq` | `DOUBLE` | yes |  | KDJ指标-CLOSE, HIGH, LOW, N=9, M1=3, M2=3 |
| `kdj_d_hfq` | `DOUBLE` | yes |  | KDJ指标-CLOSE, HIGH, LOW, N=9, M1=3, M2=3 |
| `kdj_d_qfq` | `DOUBLE` | yes |  | KDJ指标-CLOSE, HIGH, LOW, N=9, M1=3, M2=3 |
| `kdj_k_bfq` | `DOUBLE` | yes |  | KDJ指标-CLOSE, HIGH, LOW, N=9, M1=3, M2=3 |
| `kdj_k_hfq` | `DOUBLE` | yes |  | KDJ指标-CLOSE, HIGH, LOW, N=9, M1=3, M2=3 |
| `kdj_k_qfq` | `DOUBLE` | yes |  | KDJ指标-CLOSE, HIGH, LOW, N=9, M1=3, M2=3 |
| `ktn_down_bfq` | `DOUBLE` | yes |  | 肯特纳交易通道, N选20日，ATR选10日-CLOSE, HIGH, LOW, N=20, M=10 |
| `ktn_down_hfq` | `DOUBLE` | yes |  | 肯特纳交易通道, N选20日，ATR选10日-CLOSE, HIGH, LOW, N=20, M=10 |
| `ktn_down_qfq` | `DOUBLE` | yes |  | 肯特纳交易通道, N选20日，ATR选10日-CLOSE, HIGH, LOW, N=20, M=10 |
| `ktn_mid_bfq` | `DOUBLE` | yes |  | 肯特纳交易通道, N选20日，ATR选10日-CLOSE, HIGH, LOW, N=20, M=10 |
| `ktn_mid_hfq` | `DOUBLE` | yes |  | 肯特纳交易通道, N选20日，ATR选10日-CLOSE, HIGH, LOW, N=20, M=10 |
| `ktn_mid_qfq` | `DOUBLE` | yes |  | 肯特纳交易通道, N选20日，ATR选10日-CLOSE, HIGH, LOW, N=20, M=10 |
| `ktn_upper_bfq` | `DOUBLE` | yes |  | 肯特纳交易通道, N选20日，ATR选10日-CLOSE, HIGH, LOW, N=20, M=10 |
| `ktn_upper_hfq` | `DOUBLE` | yes |  | 肯特纳交易通道, N选20日，ATR选10日-CLOSE, HIGH, LOW, N=20, M=10 |
| `ktn_upper_qfq` | `DOUBLE` | yes |  | 肯特纳交易通道, N选20日，ATR选10日-CLOSE, HIGH, LOW, N=20, M=10 |
| `lowdays` | `DOUBLE` | yes |  | LOWRANGE(LOW)表示当前最低价是近多少周期内最低价的最小值 |
| `topdays` | `DOUBLE` | yes |  | TOPRANGE(HIGH)表示当前最高价是近多少周期内最高价的最大值 |
| `ma_bfq_10` | `DOUBLE` | yes |  | 简单移动平均-N=10 |
| `ma_bfq_20` | `DOUBLE` | yes |  | 简单移动平均-N=20 |
| `ma_bfq_250` | `DOUBLE` | yes |  | 简单移动平均-N=250 |
| `ma_bfq_30` | `DOUBLE` | yes |  | 简单移动平均-N=30 |
| `ma_bfq_5` | `DOUBLE` | yes |  | 简单移动平均-N=5 |
| `ma_bfq_60` | `DOUBLE` | yes |  | 简单移动平均-N=60 |
| `ma_bfq_90` | `DOUBLE` | yes |  | 简单移动平均-N=90 |
| `ma_hfq_10` | `DOUBLE` | yes |  | 简单移动平均-N=10 |
| `ma_hfq_20` | `DOUBLE` | yes |  | 简单移动平均-N=20 |
| `ma_hfq_250` | `DOUBLE` | yes |  | 简单移动平均-N=250 |
| `ma_hfq_30` | `DOUBLE` | yes |  | 简单移动平均-N=30 |
| `ma_hfq_5` | `DOUBLE` | yes |  | 简单移动平均-N=5 |
| `ma_hfq_60` | `DOUBLE` | yes |  | 简单移动平均-N=60 |
| `ma_hfq_90` | `DOUBLE` | yes |  | 简单移动平均-N=90 |
| `ma_qfq_10` | `DOUBLE` | yes |  | 简单移动平均-N=10 |
| `ma_qfq_20` | `DOUBLE` | yes |  | 简单移动平均-N=20 |
| `ma_qfq_250` | `DOUBLE` | yes |  | 简单移动平均-N=250 |
| `ma_qfq_30` | `DOUBLE` | yes |  | 简单移动平均-N=30 |
| `ma_qfq_5` | `DOUBLE` | yes |  | 简单移动平均-N=5 |
| `ma_qfq_60` | `DOUBLE` | yes |  | 简单移动平均-N=60 |
| `ma_qfq_90` | `DOUBLE` | yes |  | 简单移动平均-N=90 |
| `macd_bfq` | `DOUBLE` | yes |  | MACD指标-CLOSE, SHORT=12, LONG=26, M=9 |
| `macd_hfq` | `DOUBLE` | yes |  | MACD指标-CLOSE, SHORT=12, LONG=26, M=9 |
| `macd_qfq` | `DOUBLE` | yes |  | MACD指标-CLOSE, SHORT=12, LONG=26, M=9 |
| `macd_dea_bfq` | `DOUBLE` | yes |  | MACD指标-CLOSE, SHORT=12, LONG=26, M=9 |
| `macd_dea_hfq` | `DOUBLE` | yes |  | MACD指标-CLOSE, SHORT=12, LONG=26, M=9 |
| `macd_dea_qfq` | `DOUBLE` | yes |  | MACD指标-CLOSE, SHORT=12, LONG=26, M=9 |
| `macd_dif_bfq` | `DOUBLE` | yes |  | MACD指标-CLOSE, SHORT=12, LONG=26, M=9 |
| `macd_dif_hfq` | `DOUBLE` | yes |  | MACD指标-CLOSE, SHORT=12, LONG=26, M=9 |
| `macd_dif_qfq` | `DOUBLE` | yes |  | MACD指标-CLOSE, SHORT=12, LONG=26, M=9 |
| `mass_bfq` | `DOUBLE` | yes |  | 梅斯线-HIGH, LOW, N1=9, N2=25, M=6 |
| `mass_hfq` | `DOUBLE` | yes |  | 梅斯线-HIGH, LOW, N1=9, N2=25, M=6 |
| `mass_qfq` | `DOUBLE` | yes |  | 梅斯线-HIGH, LOW, N1=9, N2=25, M=6 |
| `ma_mass_bfq` | `DOUBLE` | yes |  | 梅斯线-HIGH, LOW, N1=9, N2=25, M=6 |
| `ma_mass_hfq` | `DOUBLE` | yes |  | 梅斯线-HIGH, LOW, N1=9, N2=25, M=6 |
| `ma_mass_qfq` | `DOUBLE` | yes |  | 梅斯线-HIGH, LOW, N1=9, N2=25, M=6 |
| `mfi_bfq` | `DOUBLE` | yes |  | MFI指标是成交量的RSI指标-CLOSE, HIGH, LOW, VOL, N=14 |
| `mfi_hfq` | `DOUBLE` | yes |  | MFI指标是成交量的RSI指标-CLOSE, HIGH, LOW, VOL, N=14 |
| `mfi_qfq` | `DOUBLE` | yes |  | MFI指标是成交量的RSI指标-CLOSE, HIGH, LOW, VOL, N=14 |
| `mtm_bfq` | `DOUBLE` | yes |  | 动量指标-CLOSE, N=12, M=6 |
| `mtm_hfq` | `DOUBLE` | yes |  | 动量指标-CLOSE, N=12, M=6 |
| `mtm_qfq` | `DOUBLE` | yes |  | 动量指标-CLOSE, N=12, M=6 |
| `mtmma_bfq` | `DOUBLE` | yes |  | 动量指标-CLOSE, N=12, M=6 |
| `mtmma_hfq` | `DOUBLE` | yes |  | 动量指标-CLOSE, N=12, M=6 |
| `mtmma_qfq` | `DOUBLE` | yes |  | 动量指标-CLOSE, N=12, M=6 |
| `obv_bfq` | `DOUBLE` | yes |  | 能量潮指标-CLOSE, VOL |
| `obv_hfq` | `DOUBLE` | yes |  | 能量潮指标-CLOSE, VOL |
| `obv_qfq` | `DOUBLE` | yes |  | 能量潮指标-CLOSE, VOL |
| `psy_bfq` | `DOUBLE` | yes |  | 投资者对股市涨跌产生心理波动的情绪指标-CLOSE, N=12, M=6 |
| `psy_hfq` | `DOUBLE` | yes |  | 投资者对股市涨跌产生心理波动的情绪指标-CLOSE, N=12, M=6 |
| `psy_qfq` | `DOUBLE` | yes |  | 投资者对股市涨跌产生心理波动的情绪指标-CLOSE, N=12, M=6 |
| `psyma_bfq` | `DOUBLE` | yes |  | 投资者对股市涨跌产生心理波动的情绪指标-CLOSE, N=12, M=6 |
| `psyma_hfq` | `DOUBLE` | yes |  | 投资者对股市涨跌产生心理波动的情绪指标-CLOSE, N=12, M=6 |
| `psyma_qfq` | `DOUBLE` | yes |  | 投资者对股市涨跌产生心理波动的情绪指标-CLOSE, N=12, M=6 |
| `roc_bfq` | `DOUBLE` | yes |  | 变动率指标-CLOSE, N=12, M=6 |
| `roc_hfq` | `DOUBLE` | yes |  | 变动率指标-CLOSE, N=12, M=6 |
| `roc_qfq` | `DOUBLE` | yes |  | 变动率指标-CLOSE, N=12, M=6 |
| `maroc_bfq` | `DOUBLE` | yes |  | 变动率指标-CLOSE, N=12, M=6 |
| `maroc_hfq` | `DOUBLE` | yes |  | 变动率指标-CLOSE, N=12, M=6 |
| `maroc_qfq` | `DOUBLE` | yes |  | 变动率指标-CLOSE, N=12, M=6 |
| `rsi_bfq_12` | `DOUBLE` | yes |  | RSI指标-CLOSE, N=12 |
| `rsi_bfq_24` | `DOUBLE` | yes |  | RSI指标-CLOSE, N=24 |
| `rsi_bfq_6` | `DOUBLE` | yes |  | RSI指标-CLOSE, N=6 |
| `rsi_hfq_12` | `DOUBLE` | yes |  | RSI指标-CLOSE, N=12 |
| `rsi_hfq_24` | `DOUBLE` | yes |  | RSI指标-CLOSE, N=24 |
| `rsi_hfq_6` | `DOUBLE` | yes |  | RSI指标-CLOSE, N=6 |
| `rsi_qfq_12` | `DOUBLE` | yes |  | RSI指标-CLOSE, N=12 |
| `rsi_qfq_24` | `DOUBLE` | yes |  | RSI指标-CLOSE, N=24 |
| `rsi_qfq_6` | `DOUBLE` | yes |  | RSI指标-CLOSE, N=6 |
| `taq_down_bfq` | `DOUBLE` | yes |  | 唐安奇通道(海龟)交易指标-HIGH, LOW, 20 |
| `taq_down_hfq` | `DOUBLE` | yes |  | 唐安奇通道(海龟)交易指标-HIGH, LOW, 20 |
| `taq_down_qfq` | `DOUBLE` | yes |  | 唐安奇通道(海龟)交易指标-HIGH, LOW, 20 |
| `taq_mid_bfq` | `DOUBLE` | yes |  | 唐安奇通道(海龟)交易指标-HIGH, LOW, 20 |
| `taq_mid_hfq` | `DOUBLE` | yes |  | 唐安奇通道(海龟)交易指标-HIGH, LOW, 20 |
| `taq_mid_qfq` | `DOUBLE` | yes |  | 唐安奇通道(海龟)交易指标-HIGH, LOW, 20 |
| `taq_up_bfq` | `DOUBLE` | yes |  | 唐安奇通道(海龟)交易指标-HIGH, LOW, 20 |
| `taq_up_hfq` | `DOUBLE` | yes |  | 唐安奇通道(海龟)交易指标-HIGH, LOW, 20 |
| `taq_up_qfq` | `DOUBLE` | yes |  | 唐安奇通道(海龟)交易指标-HIGH, LOW, 20 |
| `trix_bfq` | `DOUBLE` | yes |  | 三重指数平滑平均线-CLOSE, M1=12, M2=20 |
| `trix_hfq` | `DOUBLE` | yes |  | 三重指数平滑平均线-CLOSE, M1=12, M2=20 |
| `trix_qfq` | `DOUBLE` | yes |  | 三重指数平滑平均线-CLOSE, M1=12, M2=20 |
| `trma_bfq` | `DOUBLE` | yes |  | 三重指数平滑平均线-CLOSE, M1=12, M2=20 |
| `trma_hfq` | `DOUBLE` | yes |  | 三重指数平滑平均线-CLOSE, M1=12, M2=20 |
| `trma_qfq` | `DOUBLE` | yes |  | 三重指数平滑平均线-CLOSE, M1=12, M2=20 |
| `vr_bfq` | `DOUBLE` | yes |  | VR容量比率-CLOSE, VOL, M1=26 |
| `vr_hfq` | `DOUBLE` | yes |  | VR容量比率-CLOSE, VOL, M1=26 |
| `vr_qfq` | `DOUBLE` | yes |  | VR容量比率-CLOSE, VOL, M1=26 |
| `wr_bfq` | `DOUBLE` | yes |  | W&R 威廉指标-CLOSE, HIGH, LOW, N=10, N1=6 |
| `wr_hfq` | `DOUBLE` | yes |  | W&R 威廉指标-CLOSE, HIGH, LOW, N=10, N1=6 |
| `wr_qfq` | `DOUBLE` | yes |  | W&R 威廉指标-CLOSE, HIGH, LOW, N=10, N1=6 |
| `wr1_bfq` | `DOUBLE` | yes |  | W&R 威廉指标-CLOSE, HIGH, LOW, N=10, N1=6 |
| `wr1_hfq` | `DOUBLE` | yes |  | W&R 威廉指标-CLOSE, HIGH, LOW, N=10, N1=6 |
| `wr1_qfq` | `DOUBLE` | yes |  | W&R 威廉指标-CLOSE, HIGH, LOW, N=10, N1=6 |
| `xsii_td1_bfq` | `DOUBLE` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |
| `xsii_td1_hfq` | `DOUBLE` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |
| `xsii_td1_qfq` | `DOUBLE` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |
| `xsii_td2_bfq` | `DOUBLE` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |
| `xsii_td2_hfq` | `DOUBLE` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |
| `xsii_td2_qfq` | `DOUBLE` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |
| `xsii_td3_bfq` | `DOUBLE` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |
| `xsii_td3_hfq` | `DOUBLE` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |
| `xsii_td3_qfq` | `DOUBLE` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |
| `xsii_td4_bfq` | `DOUBLE` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |
| `xsii_td4_hfq` | `DOUBLE` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |
| `xsii_td4_qfq` | `DOUBLE` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |

## stock_moneyflow

- API: `moneyflow`
- Official doc: https://tushare.pro/document/2?doc_id=170
- Checked at: `2026-11-21`
- SDK version: `1.4.29`
- Fetch plan: `kind`=`by_trade_date`; `params`=`trade_date`
- Write strategy: `type`=`upsert`; `key`=`trade_date`, `ts_code`
- Note: Tushare moneyflow is tiered by trade size (small/medium/large/extra-large buy and sell volume/amount). The legacy THS moneyflow tables were removed from this cache.
- Note: Verified with this token on 2026-11-21: 20200103 returned 3,741 rows and 20261120 returned 5,580 rows, so the historical range needed for the 2020 backfill is available.

| Field | DuckDB type | Nullable | Source format | Description |
| --- | --- | --- | --- | --- |
| `ts_code` | `VARCHAR` | no |  | TS代码 |
| `trade_date` | `DATE` | no | YYYYMMDD | 交易日期 |
| `buy_sm_vol` | `DOUBLE` | yes |  | 小单买入量（手） |
| `buy_sm_amount` | `DOUBLE` | yes |  | 小单买入金额（万元） |
| `sell_sm_vol` | `DOUBLE` | yes |  | 小单卖出量（手） |
| `sell_sm_amount` | `DOUBLE` | yes |  | 小单卖出金额（万元） |
| `buy_md_vol` | `DOUBLE` | yes |  | 中单买入量（手） |
| `buy_md_amount` | `DOUBLE` | yes |  | 中单买入金额（万元） |
| `sell_md_vol` | `DOUBLE` | yes |  | 中单卖出量（手） |
| `sell_md_amount` | `DOUBLE` | yes |  | 中单卖出金额（万元） |
| `buy_lg_vol` | `DOUBLE` | yes |  | 大单买入量（手） |
| `buy_lg_amount` | `DOUBLE` | yes |  | 大单买入金额（万元） |
| `sell_lg_vol` | `DOUBLE` | yes |  | 大单卖出量（手） |
| `sell_lg_amount` | `DOUBLE` | yes |  | 大单卖出金额（万元） |
| `buy_elg_vol` | `DOUBLE` | yes |  | 特大单买入量（手） |
| `buy_elg_amount` | `DOUBLE` | yes |  | 特大单买入金额（万元） |
| `sell_elg_vol` | `DOUBLE` | yes |  | 特大单卖出量（手） |
| `sell_elg_amount` | `DOUBLE` | yes |  | 特大单卖出金额（万元） |
| `net_mf_vol` | `DOUBLE` | yes |  | 净流入量（手） |
| `net_mf_amount` | `DOUBLE` | yes |  | 净流入额（万元） |

## index_idx_factor_pro

- API: `idx_factor_pro`
- Official doc: https://tushare.pro/wctapi/documents/358.md
- Checked at: `2026-06-26`
- SDK version: `1.4.29`
- Fetch plan: `kind`=`by_trade_date_and_code_list`; `params`=`ts_code`, `start_date`, `end_date`; `code_source`=`INDEX_LIST_PATH enabled rows`
- Write strategy: `type`=`upsert`; `key`=`trade_date`, `ts_code`
- Note: Fields are frozen from official Tushare output docs and SDK samples checked on 2026-06-26; optional output fields are included and should be requested explicitly with the Tushare fields parameter. DuckDB types are chosen explicitly from official docs plus source semantics, not inferred mechanically from pandas dtype.
- Note: SDK sample used 000001.SH over 20260529-20260626.

| Field | DuckDB type | Nullable | Source format | Description |
| --- | --- | --- | --- | --- |
| `ts_code` | `VARCHAR` | no |  | 指数代码 |
| `trade_date` | `DATE` | no | YYYYMMDD | 交易日期 |
| `open` | `DOUBLE` | yes |  | 开盘价 |
| `high` | `DOUBLE` | yes |  | 最高价 |
| `low` | `DOUBLE` | yes |  | 最低价 |
| `close` | `DOUBLE` | yes |  | 收盘价 |
| `pre_close` | `DOUBLE` | yes |  | 昨收价 |
| `change` | `DOUBLE` | yes |  | 涨跌额 |
| `pct_change` | `DOUBLE` | yes |  | 涨跌幅 （未复权，如果是复权请用 通用行情接口 ） |
| `vol` | `DOUBLE` | yes |  | 成交量 （手） |
| `amount` | `DOUBLE` | yes |  | 成交额 （千元） |
| `asi_bfq` | `DOUBLE` | yes |  | 振动升降指标-OPEN, CLOSE, HIGH, LOW, M1=26, M2=10 |
| `asit_bfq` | `DOUBLE` | yes |  | 振动升降指标-OPEN, CLOSE, HIGH, LOW, M1=26, M2=10 |
| `atr_bfq` | `DOUBLE` | yes |  | 真实波动N日平均值-CLOSE, HIGH, LOW, N=20 |
| `bbi_bfq` | `DOUBLE` | yes |  | BBI多空指标-CLOSE, M1=3, M2=6, M3=12, M4=20 |
| `bias1_bfq` | `DOUBLE` | yes |  | BIAS乖离率-CLOSE, L1=6, L2=12, L3=24 |
| `bias2_bfq` | `DOUBLE` | yes |  | BIAS乖离率-CLOSE, L1=6, L2=12, L3=24 |
| `bias3_bfq` | `DOUBLE` | yes |  | BIAS乖离率-CLOSE, L1=6, L2=12, L3=24 |
| `boll_lower_bfq` | `DOUBLE` | yes |  | BOLL指标，布林带-CLOSE, N=20, P=2 |
| `boll_mid_bfq` | `DOUBLE` | yes |  | BOLL指标，布林带-CLOSE, N=20, P=2 |
| `boll_upper_bfq` | `DOUBLE` | yes |  | BOLL指标，布林带-CLOSE, N=20, P=2 |
| `brar_ar_bfq` | `DOUBLE` | yes |  | BRAR情绪指标-OPEN, CLOSE, HIGH, LOW, M1=26 |
| `brar_br_bfq` | `DOUBLE` | yes |  | BRAR情绪指标-OPEN, CLOSE, HIGH, LOW, M1=26 |
| `cci_bfq` | `DOUBLE` | yes |  | 顺势指标又叫CCI指标-CLOSE, HIGH, LOW, N=14 |
| `cr_bfq` | `DOUBLE` | yes |  | CR价格动量指标-CLOSE, HIGH, LOW, N=20 |
| `dfma_dif_bfq` | `DOUBLE` | yes |  | 平行线差指标-CLOSE, N1=10, N2=50, M=10 |
| `dfma_difma_bfq` | `DOUBLE` | yes |  | 平行线差指标-CLOSE, N1=10, N2=50, M=10 |
| `dmi_adx_bfq` | `DOUBLE` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `dmi_adxr_bfq` | `DOUBLE` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `dmi_mdi_bfq` | `DOUBLE` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `dmi_pdi_bfq` | `DOUBLE` | yes |  | 动向指标-CLOSE, HIGH, LOW, M1=14, M2=6 |
| `downdays` | `DOUBLE` | yes |  | 连跌天数 |
| `updays` | `DOUBLE` | yes |  | 连涨天数 |
| `dpo_bfq` | `DOUBLE` | yes |  | 区间震荡线-CLOSE, M1=20, M2=10, M3=6 |
| `madpo_bfq` | `DOUBLE` | yes |  | 区间震荡线-CLOSE, M1=20, M2=10, M3=6 |
| `ema_bfq_10` | `DOUBLE` | yes |  | 指数移动平均-N=10 |
| `ema_bfq_20` | `DOUBLE` | yes |  | 指数移动平均-N=20 |
| `ema_bfq_250` | `DOUBLE` | yes |  | 指数移动平均-N=250 |
| `ema_bfq_30` | `DOUBLE` | yes |  | 指数移动平均-N=30 |
| `ema_bfq_5` | `DOUBLE` | yes |  | 指数移动平均-N=5 |
| `ema_bfq_60` | `DOUBLE` | yes |  | 指数移动平均-N=60 |
| `ema_bfq_90` | `DOUBLE` | yes |  | 指数移动平均-N=90 |
| `emv_bfq` | `DOUBLE` | yes |  | 简易波动指标-HIGH, LOW, VOL, N=14, M=9 |
| `maemv_bfq` | `DOUBLE` | yes |  | 简易波动指标-HIGH, LOW, VOL, N=14, M=9 |
| `expma_12_bfq` | `DOUBLE` | yes |  | EMA指数平均数指标-CLOSE, N1=12, N2=50 |
| `expma_50_bfq` | `DOUBLE` | yes |  | EMA指数平均数指标-CLOSE, N1=12, N2=50 |
| `kdj_bfq` | `DOUBLE` | yes |  | KDJ指标-CLOSE, HIGH, LOW, N=9, M1=3, M2=3 |
| `kdj_d_bfq` | `DOUBLE` | yes |  | KDJ指标-CLOSE, HIGH, LOW, N=9, M1=3, M2=3 |
| `kdj_k_bfq` | `DOUBLE` | yes |  | KDJ指标-CLOSE, HIGH, LOW, N=9, M1=3, M2=3 |
| `ktn_down_bfq` | `DOUBLE` | yes |  | 肯特纳交易通道, N选20日，ATR选10日-CLOSE, HIGH, LOW, N=20, M=10 |
| `ktn_mid_bfq` | `DOUBLE` | yes |  | 肯特纳交易通道, N选20日，ATR选10日-CLOSE, HIGH, LOW, N=20, M=10 |
| `ktn_upper_bfq` | `DOUBLE` | yes |  | 肯特纳交易通道, N选20日，ATR选10日-CLOSE, HIGH, LOW, N=20, M=10 |
| `lowdays` | `DOUBLE` | yes |  | LOWRANGE(LOW)表示当前最低价是近多少周期内最低价的最小值 |
| `topdays` | `DOUBLE` | yes |  | TOPRANGE(HIGH)表示当前最高价是近多少周期内最高价的最大值 |
| `ma_bfq_10` | `DOUBLE` | yes |  | 简单移动平均-N=10 |
| `ma_bfq_20` | `DOUBLE` | yes |  | 简单移动平均-N=20 |
| `ma_bfq_250` | `DOUBLE` | yes |  | 简单移动平均-N=250 |
| `ma_bfq_30` | `DOUBLE` | yes |  | 简单移动平均-N=30 |
| `ma_bfq_5` | `DOUBLE` | yes |  | 简单移动平均-N=5 |
| `ma_bfq_60` | `DOUBLE` | yes |  | 简单移动平均-N=60 |
| `ma_bfq_90` | `DOUBLE` | yes |  | 简单移动平均-N=90 |
| `macd_bfq` | `DOUBLE` | yes |  | MACD指标-CLOSE, SHORT=12, LONG=26, M=9 |
| `macd_dea_bfq` | `DOUBLE` | yes |  | MACD指标-CLOSE, SHORT=12, LONG=26, M=9 |
| `macd_dif_bfq` | `DOUBLE` | yes |  | MACD指标-CLOSE, SHORT=12, LONG=26, M=9 |
| `mass_bfq` | `DOUBLE` | yes |  | 梅斯线-HIGH, LOW, N1=9, N2=25, M=6 |
| `ma_mass_bfq` | `DOUBLE` | yes |  | 梅斯线-HIGH, LOW, N1=9, N2=25, M=6 |
| `mfi_bfq` | `DOUBLE` | yes |  | MFI指标是成交量的RSI指标-CLOSE, HIGH, LOW, VOL, N=14 |
| `mtm_bfq` | `DOUBLE` | yes |  | 动量指标-CLOSE, N=12, M=6 |
| `mtmma_bfq` | `DOUBLE` | yes |  | 动量指标-CLOSE, N=12, M=6 |
| `obv_bfq` | `DOUBLE` | yes |  | 能量潮指标-CLOSE, VOL |
| `psy_bfq` | `DOUBLE` | yes |  | 投资者对股市涨跌产生心理波动的情绪指标-CLOSE, N=12, M=6 |
| `psyma_bfq` | `DOUBLE` | yes |  | 投资者对股市涨跌产生心理波动的情绪指标-CLOSE, N=12, M=6 |
| `roc_bfq` | `DOUBLE` | yes |  | 变动率指标-CLOSE, N=12, M=6 |
| `maroc_bfq` | `DOUBLE` | yes |  | 变动率指标-CLOSE, N=12, M=6 |
| `rsi_bfq_12` | `DOUBLE` | yes |  | RSI指标-CLOSE, N=12 |
| `rsi_bfq_24` | `DOUBLE` | yes |  | RSI指标-CLOSE, N=24 |
| `rsi_bfq_6` | `DOUBLE` | yes |  | RSI指标-CLOSE, N=6 |
| `taq_down_bfq` | `DOUBLE` | yes |  | 唐安奇通道(海龟)交易指标-HIGH, LOW, 20 |
| `taq_mid_bfq` | `DOUBLE` | yes |  | 唐安奇通道(海龟)交易指标-HIGH, LOW, 20 |
| `taq_up_bfq` | `DOUBLE` | yes |  | 唐安奇通道(海龟)交易指标-HIGH, LOW, 20 |
| `trix_bfq` | `DOUBLE` | yes |  | 三重指数平滑平均线-CLOSE, M1=12, M2=20 |
| `trma_bfq` | `DOUBLE` | yes |  | 三重指数平滑平均线-CLOSE, M1=12, M2=20 |
| `vr_bfq` | `DOUBLE` | yes |  | VR容量比率-CLOSE, VOL, M1=26 |
| `wr_bfq` | `DOUBLE` | yes |  | W&R 威廉指标-CLOSE, HIGH, LOW, N=10, N1=6 |
| `wr1_bfq` | `DOUBLE` | yes |  | W&R 威廉指标-CLOSE, HIGH, LOW, N=10, N1=6 |
| `xsii_td1_bfq` | `DOUBLE` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |
| `xsii_td2_bfq` | `DOUBLE` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |
| `xsii_td3_bfq` | `DOUBLE` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |
| `xsii_td4_bfq` | `DOUBLE` | yes |  | 薛斯通道II-CLOSE, HIGH, LOW, N=102, M=7 |

## index_ths

- API: `ths_index`
- Official doc: https://tushare.pro/wctapi/documents/259.md
- Checked at: `2026-06-26`
- SDK version: `1.4.29`
- Fetch plan: `kind`=`refresh_current`; `params`=
- Write strategy: `type`=`replace_all`; `key`=`ts_code`; `transaction_group`=`ths_classification`
- Note: Fields are frozen from official Tushare output docs and SDK samples checked on 2026-06-26; optional output fields are included and should be requested explicitly with the Tushare fields parameter. DuckDB types are chosen explicitly from official docs plus source semantics, not inferred mechanically from pandas dtype.
- Note: Current classification list only; no historical snapshot columns are added.

| Field | DuckDB type | Nullable | Source format | Description |
| --- | --- | --- | --- | --- |
| `ts_code` | `VARCHAR` | no |  | 代码 |
| `name` | `VARCHAR` | yes |  | 名称 |
| `count` | `BIGINT` | yes |  | 成分个数 |
| `exchange` | `VARCHAR` | yes |  | 交易所 |
| `list_date` | `DATE` | yes | YYYYMMDD | 上市日期 |
| `type` | `VARCHAR` | yes |  | N概念指数S特色指数 |

## index_ths_daily

- API: `ths_daily`
- Official doc: https://tushare.pro/wctapi/documents/260.md
- Checked at: `2026-06-26`
- SDK version: `1.4.29`
- Fetch plan: `kind`=`by_trade_date`; `params`=`trade_date`
- Write strategy: `type`=`upsert`; `key`=`trade_date`, `ts_code`
- Note: Fields are frozen from official Tushare output docs and SDK samples checked on 2026-06-26; optional output fields are included and should be requested explicitly with the Tushare fields parameter. DuckDB types are chosen explicitly from official docs plus source semantics, not inferred mechanically from pandas dtype.
- Note: Cache fetches use pro.ths_daily(trade_date=YYYYMMDD) without ts_code so the Rust side matches the Python function's whole-market daily shape.
- Note: Optional total_mv and float_mv were confirmed with an explicit fields request in SDK 1.4.29.

| Field | DuckDB type | Nullable | Source format | Description |
| --- | --- | --- | --- | --- |
| `ts_code` | `VARCHAR` | no |  | TS指数代码 |
| `trade_date` | `DATE` | no | YYYYMMDD | 交易日 |
| `open` | `DOUBLE` | yes |  | 开盘点位 |
| `high` | `DOUBLE` | yes |  | 最高点位 |
| `low` | `DOUBLE` | yes |  | 最低点位 |
| `close` | `DOUBLE` | yes |  | 收盘点位 |
| `pre_close` | `DOUBLE` | yes |  | 昨日收盘点 |
| `avg_price` | `DOUBLE` | yes |  | 平均价 |
| `change` | `DOUBLE` | yes |  | 涨跌点位 |
| `pct_change` | `DOUBLE` | yes |  | 涨跌幅 |
| `vol` | `DOUBLE` | yes |  | 成交量（手） |
| `turnover_rate` | `DOUBLE` | yes |  | 换手率（%） |
| `total_mv` | `DOUBLE` | yes |  | 总市值（元） |
| `float_mv` | `DOUBLE` | yes |  | 流通市值（元） |

## index_ths_member

- API: `ths_member`
- Official doc: https://tushare.pro/wctapi/documents/261.md
- Checked at: `2026-06-26`
- SDK version: `1.4.29`
- Fetch plan: `kind`=`refresh_current_by_ths_code`; `params`=`ts_code`; `code_source`=`index_ths`
- Write strategy: `type`=`replace_all`; `key`=`ts_code`, `con_code`; `transaction_group`=`ths_classification`
- Note: Fields are frozen from official Tushare output docs and SDK samples checked on 2026-06-26; optional output fields are included and should be requested explicitly with the Tushare fields parameter. DuckDB types are chosen explicitly from official docs plus source semantics, not inferred mechanically from pandas dtype.
- Note: Current membership relation only; no daily membership snapshot is stored.
- Note: Optional weight, in_date, out_date, and is_new are included. SDK 1.4.29 explicit fields sample returned is_new and returned weight/in_date/out_date as all NULL for the sampled THS index.

| Field | DuckDB type | Nullable | Source format | Description |
| --- | --- | --- | --- | --- |
| `ts_code` | `VARCHAR` | no |  | 指数代码 |
| `con_code` | `VARCHAR` | no |  | 股票代码 |
| `con_name` | `VARCHAR` | yes |  | 股票名称 |
| `weight` | `DOUBLE` | yes |  | 权重(暂无) |
| `in_date` | `DATE` | yes | YYYYMMDD | 纳入日期(暂无) |
| `out_date` | `DATE` | yes | YYYYMMDD | 剔除日期(暂无) |
| `is_new` | `VARCHAR` | yes |  | 是否最新Y是N否 |

## index_sw_classify

- API: `index_classify`
- Official doc: https://tushare.pro/document/2?doc_id=181
- Checked at: `2026-11-21`
- SDK version: `1.4.29`
- Fetch plan: `kind`=`refresh_current`; `params`=`src`
- Write strategy: `type`=`replace_all`; `key`=`index_code`; `transaction_group`=`sw_classification`
- Note: The refresh stage requests src=SW2021 without a level filter, returning 511 L1/L2/L3 classification rows with parent_code links.
- Note: SW2021 L2 contains 134 rows; 124 are is_pub=1 and all 124 have idx_factor_pro data from 2020-01-03 onward. The 10 is_pub=0 industries return no index factor rows.

| Field | DuckDB type | Nullable | Source format | Description |
| --- | --- | --- | --- | --- |
| `index_code` | `VARCHAR` | no |  | 指数代码 |
| `industry_name` | `VARCHAR` | yes |  | 行业名称 |
| `level` | `VARCHAR` | yes |  | 行业层级 |
| `industry_code` | `VARCHAR` | yes |  | 行业代码 |
| `is_pub` | `VARCHAR` | yes |  | 是否发布指数Y是N否 |
| `parent_code` | `VARCHAR` | yes |  | 父级行业代码 |
| `src` | `VARCHAR` | yes |  | 行业分类来源 |

## index_sw_member

- API: `index_member_all`
- Official doc: https://tushare.pro/document/2?doc_id=335
- Checked at: `2026-11-21`
- SDK version: `1.4.29`
- Fetch plan: `kind`=`refresh_current`; `params`=`is_new`
- Write strategy: `type`=`replace_all`; `key`=`ts_code`, `in_date`; `transaction_group`=`sw_classification`
- Note: The refresh stage unions two snapshots, is_new=Y (current members) and is_new=N (expired members), so in_date/out_date support point-in-time industry attribution. The 2026-11-21 refresh returned 7,932 unique rows (5,919 current + 2,013 expired).
- Note: in_date/out_date are normalized to DATE; out_date is NULL for current members. (ts_code, in_date) was unique in the observed union, so it is used as the snapshot key.
- Note: This table is NOT filtered to a single date; keep the full interval history for backfills.

| Field | DuckDB type | Nullable | Source format | Description |
| --- | --- | --- | --- | --- |
| `l1_code` | `VARCHAR` | yes |  | 一级行业代码 |
| `l1_name` | `VARCHAR` | yes |  | 一级行业名称 |
| `l2_code` | `VARCHAR` | yes |  | 二级行业代码 |
| `l2_name` | `VARCHAR` | yes |  | 二级行业名称 |
| `l3_code` | `VARCHAR` | yes |  | 三级行业代码 |
| `l3_name` | `VARCHAR` | yes |  | 三级行业名称 |
| `ts_code` | `VARCHAR` | no |  | 股票代码 |
| `name` | `VARCHAR` | yes |  | 股票名称 |
| `in_date` | `DATE` | no | YYYYMMDD | 纳入日期 |
| `out_date` | `DATE` | yes | YYYYMMDD | 剔除日期 |
| `is_new` | `VARCHAR` | yes |  | 是否最新Y是N否 |

## index_weight

- API: `index_weight`
- Official doc: https://tushare.pro/document/2?doc_id=96
- Checked at: `2026-11-22`
- SDK version: `1.4.29`
- Fetch plan: `kind`=`refresh_weight_history`; `params`=`index_code`, `start_date`, `end_date`
- Write strategy: `type`=`upsert`; `key`=`index_code`, `trade_date`, `con_code`
- Note: Monthly constituent weights for configured market indices (category=market). The refresh stage walks 2020-01-01 through today in yearly chunks so Tushare's 100,000-row offset limit is never reached.
- Note: Verified on 2026-11-22: 000300.SH has 82 monthly snapshots from 2020-01-23; 000688.SH starts 2020-07-31, 399303.SZ 2022-06-30, 899050.BJ 2022-11-30, and 930050.CSI 2024-01-31.

| Field | DuckDB type | Nullable | Source format | Description |
| --- | --- | --- | --- | --- |
| `index_code` | `VARCHAR` | no |  | 指数代码 |
| `trade_date` | `DATE` | no | YYYYMMDD | 交易日期 |
| `con_code` | `VARCHAR` | no |  | 成分股票代码 |
| `weight` | `DOUBLE` | yes |  | 权重（%） |

## index_limit_cpt_list

- API: `limit_cpt_list`
- Official doc: https://tushare.pro/wctapi/documents/357.md
- Checked at: `2026-06-26`
- SDK version: `1.4.29`
- Fetch plan: `kind`=`by_trade_date`; `params`=`trade_date`
- Write strategy: `type`=`delete_insert_by_date`; `date_field`=`trade_date`; `natural_key_candidate`=`trade_date`, `rank`, `ts_code`
- Note: Fields are frozen from official Tushare output docs and SDK samples checked on 2026-06-26; optional output fields are included and should be requested explicitly with the Tushare fields parameter. DuckDB types are chosen explicitly from official docs plus source semantics, not inferred mechanically from pandas dtype.
- Note: No stable unique key is guaranteed by the docs; v1 uses date-slice delete then insert.
- Doc mismatch: Official docs describe cons_nums as int, but SDK 1.4.29 sample returned string values; v1 stores cons_nums as text.
- Doc mismatch: Official docs describe rank as str, but SDK 1.4.29 sample returned integer values; v1 stores rank as integer.

| Field | DuckDB type | Nullable | Source format | Description |
| --- | --- | --- | --- | --- |
| `ts_code` | `VARCHAR` | no |  | 板块代码 |
| `name` | `VARCHAR` | yes |  | 板块名称 |
| `trade_date` | `DATE` | no | YYYYMMDD | 交易日期 |
| `days` | `BIGINT` | yes |  | 上榜天数 |
| `up_stat` | `VARCHAR` | yes |  | 连板高度 |
| `cons_nums` | `VARCHAR` | yes |  | 连板家数 |
| `up_nums` | `BIGINT` | yes |  | 涨停家数 |
| `pct_chg` | `DOUBLE` | yes |  | 涨跌幅% |
| `rank` | `BIGINT` | yes |  | 板块热点排名 |

## stock_limit_list_ths

- API: `limit_list_ths`
- Official doc: https://tushare.pro/wctapi/documents/355.md
- Checked at: `2026-06-26`
- SDK version: `1.4.29`
- Fetch plan: `kind`=`by_trade_date`; `params`=`trade_date`
- Write strategy: `type`=`upsert`; `key`=`trade_date`, `ts_code`
- Note: Fields are frozen from official Tushare output docs and SDK samples checked on 2026-06-26; optional output fields are included and should be requested explicitly with the Tushare fields parameter. DuckDB types are chosen explicitly from official docs plus source semantics, not inferred mechanically from pandas dtype.
- Note: Optional first_lu_time, last_lu_time, first_ld_time, last_ld_time, rise_rate, and sum_float are included. SDK 1.4.29 explicit fields sample returned first_ld_time, last_ld_time, lu_limit_order, and sum_float as all NULL for the sampled date.

| Field | DuckDB type | Nullable | Source format | Description |
| --- | --- | --- | --- | --- |
| `trade_date` | `DATE` | no | YYYYMMDD | 交易日期 |
| `ts_code` | `VARCHAR` | no |  | 股票代码 |
| `name` | `VARCHAR` | yes |  | 股票名称 |
| `price` | `DOUBLE` | yes |  | 收盘价(元) |
| `pct_chg` | `DOUBLE` | yes |  | 涨跌幅% |
| `open_num` | `BIGINT` | yes |  | 打开次数 |
| `lu_desc` | `VARCHAR` | yes |  | 涨停原因 |
| `limit_type` | `VARCHAR` | yes |  | 板单类别 |
| `tag` | `VARCHAR` | yes |  | 涨停标签 |
| `status` | `VARCHAR` | yes |  | 涨停状态（N连板、一字板） |
| `first_lu_time` | `TIMESTAMP` | yes | YYYY-MM-DD HH:MM:SS | 首次涨停时间 |
| `last_lu_time` | `TIMESTAMP` | yes | YYYY-MM-DD HH:MM:SS | 最后涨停时间 |
| `first_ld_time` | `TIMESTAMP` | yes | YYYY-MM-DD HH:MM:SS | 首次跌停时间 |
| `last_ld_time` | `TIMESTAMP` | yes | YYYY-MM-DD HH:MM:SS | 最后跌停时间 |
| `limit_order` | `DOUBLE` | yes |  | 封单量(元或个) |
| `limit_amount` | `DOUBLE` | yes |  | 封单额(元或个) |
| `turnover_rate` | `DOUBLE` | yes |  | 换手率% |
| `free_float` | `DOUBLE` | yes |  | 实际流通(元或个) |
| `lu_limit_order` | `DOUBLE` | yes |  | 最大封单(元或个) |
| `limit_up_suc_rate` | `DOUBLE` | yes |  | 近一年涨停封板率 |
| `turnover` | `DOUBLE` | yes |  | 成交额 |
| `rise_rate` | `DOUBLE` | yes |  | 涨速 |
| `sum_float` | `DOUBLE` | yes |  | 总市值（亿元） |
| `market_type` | `VARCHAR` | yes |  | 股票类型：HS沪深主板、GEM创业板、STAR科创板 |
