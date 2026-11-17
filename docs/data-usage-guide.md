# v3 DuckDB 使用指南

计算来源表沿用 Tushare 接口名，计算结果表使用 `calc_` 前缀。所有表位于同一个 DuckDB 文件；表结构由 `docs/schema-design.md` 和 Rust schema 初始化代码共同定义。

## 表选择

| 用途 | 表 |
| --- | --- |
| Tushare 原始缓存 | `stock_basic`, `stock_stk_factor_pro`, `stock_moneyflow_ths`, `index_idx_factor_pro`, `index_ths`, `index_ths_daily`, `index_moneyflow_cnt_ths`, `index_ths_member`, `index_limit_cpt_list`, `stock_limit_list_ths` |
| 已完成周/月 OHLC | `calc_period_bars` |
| 已完成周期递推状态 | `calc_indicator_state` |
| 每个交易日的日/周/月 MACD 与周/月 KDJ as-of | `calc_daily_indicators` |
| 每日 CZSC 快照 | `calc_daily_czsc_structure` |
| 每日 rolling 因子 | `calc_daily_rolling_factors` |
| 每日左峰结构 | `calc_daily_left_peak` |

计算结果按 `source_dataset`, `ts_code`, 日期和 `calc_version` 隔离。查询计算表时始终带上来源和版本条件。

当前默认版本为 `v3-macd-kdj-czsc-1`；版本表示完整计算契约，包含价格口径、参数和结构规则。

## EOD 查询示例

```sql
select
  source_dataset,
  ts_code,
  trade_date,
  daily_macd_dif_asof,
  daily_macd_dea_asof,
  daily_macd_hist_x2_asof,
  weekly_macd_dif_asof,
  weekly_macd_dea_asof,
  weekly_kdj_k_asof,
  weekly_kdj_d_asof,
  weekly_kdj_j_asof,
  monthly_macd_dif_asof,
  monthly_macd_dea_asof,
  monthly_kdj_j_asof
from calc_daily_indicators
where source_dataset = 'stock_stk_factor_pro'
  and ts_code = '000001.SZ'
  and trade_date = date '2026-11-13'
  and calc_version = 'v3-macd-kdj-czsc-1';
```

`*_macd_hist_x2_asof` 是 `2 * (dif - dea)`。KDJ 使用完整窗口和固定 `1/3` 平滑，窗口不足时为 `NULL`。

## 周/月状态与盘中推演

`calc_indicator_state` 只保存已完成周期。周/月未完成周期的 as-of 值写入 `calc_daily_indicators`，不会推进完成状态。读取最近完成状态：

```sql
select *
from calc_indicator_state
where source_dataset = 'stock_stk_factor_pro'
  and ts_code = '000001.SZ'
  and period_type in ('weekly', 'monthly')
  and period_end_date < date '2026-11-13'
  and calc_version = 'v3-macd-kdj-czsc-1'
order by period_type, period_end_date desc;
```

盘中只能从最近完成状态用当前价格推演一次，不能把前一天的未完成周/月 as-of 再次递推。EMA 参数为 `alpha12=2/13`、`alpha26=2/27`、`alpha9=2/10`：

```sql
with state as (
  select *
  from calc_indicator_state
  where source_dataset = 'stock_stk_factor_pro'
    and ts_code = '000001.SZ'
    and period_type = 'weekly'
    and period_end_date < date '2026-11-13'
    and calc_version = 'v3-macd-kdj-czsc-1'
  order by period_end_date desc
  limit 1
), live as (select 10.23::double as price), ema as (
  select
    macd_ema12 * (11.0 / 13.0) + price * (2.0 / 13.0) as ema12_live,
    macd_ema26 * (25.0 / 27.0) + price * (2.0 / 27.0) as ema26_live,
    macd_dea as dea_prev
  from state cross join live
), macd as (
  select *, ema12_live - ema26_live as dif_live from ema
)
select
  ema12_live,
  ema26_live,
  dif_live,
  dea_prev * 0.8 + dif_live * 0.2 as dea_live,
  2.0 * (dif_live - (dea_prev * 0.8 + dif_live * 0.2)) as hist_x2_live
from macd;
```

## CZSC、rolling 与缺失值

`calc_daily_czsc_structure` 为每个存在源日线的 `(source_dataset, ts_code, trade_date)` 保留 point-in-time 行。`*_source_bar_valid=false` 表示当天 OHLC 无效；其余结构字段沿用此前有效状态。rolling 窗口不足、KDJ 样本不足、分母为零和无法定位左峰时使用 `NULL`，不要直接当成零。

## 覆盖检查

```sql
select source_dataset, min(trade_date), max(trade_date), count(*)
from calc_daily_indicators
group by source_dataset;

select source_dataset, period_type, min(period_end_date), max(period_end_date), count(*)
from calc_indicator_state
group by source_dataset, period_type;
```

DuckDB CLI 的 `status` 命令会输出来源表和计算表的行数；因子回补时先准备包含 warmup 历史的 JSONL，再使用 `stock-cache-calc --changed-from ... --write`。
