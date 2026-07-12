# Data Usage Guide

本文面向数据库消费者，说明如何读取缓存表、计算表、中间递推状态，以及如何用数据库预存的长周期指标支持 EOD 和盘中使用。

## 表选择

优先按使用场景选表：

| 场景 | 推荐表 | 说明 |
| --- | --- | --- |
| 查询源行情和 Tushare 指标 | `stock_stk_factor_pro` | 股票日线、前复权价、Tushare 自带 qfq/bfq/hfq 指标 |
| 股票 EOD 周/月 MACD | `stock_daily_asof_indicators` | 每个交易日已经落好的 daily/weekly/monthly as-of 快照 |
| 板块/指数 EOD 周/月 MACD | `index_daily_asof_indicators` | 同花顺板块指数和默认市场指数的 as-of 快照 |
| 盘中推演股票 MACD | `stock_period_indicator_state` | 读取上一完成周期状态，再用盘中前复权价递推一次 |
| 盘中推演板块/指数 MACD | `index_period_indicator_state` | 读取上一完成周期状态，再用当前指数价格递推一次 |
| 股票滚动因子 | `stock_daily_rolling_factors` | 25/144/220 日均线、区间位置、量能、换手、压缩度 |
| 左峰结构 | `stock_daily_left_peak` | 左侧压力位、突破结构、第一阴线锚点 |
| CZSC 结构 | `stock_daily_czsc_structure` | 严格 point-in-time 的完成笔、中枢、近 7 笔峰值和峰值后首根阴线 |

所有计算表都带 `calc_version`。查询时必须显式带上版本，避免未来版本并存时混用口径。

当前版本：

```text
macd_qfq_12_26_9_v1
rolling_qfq_v1
left_peak_qfq_v1
czsc_structure_qfq_min6_r7_v1
```

## EOD 使用

日终选股或回测应优先读取 as-of 表，不需要重新从三年日线窗口聚合周线/月线。

股票示例：

```sql
select
  s.ts_code,
  s.trade_date,
  s.close_qfq,
  a.daily_dif_asof,
  a.daily_dea_asof,
  a.daily_hist_x2_asof,
  a.weekly_dif_asof,
  a.weekly_dea_asof,
  a.weekly_hist_x2_asof,
  a.monthly_dif_asof,
  a.monthly_dea_asof,
  a.monthly_hist_x2_asof,
  r.ma25_qfq,
  r.ma144_qfq,
  r.ma220_qfq,
  r.position_90d,
  r.volume_to_ma20_ratio,
  l.left_peak_date,
  l.left_peak_high,
  l.is_valid as left_peak_valid,
  c.status as czsc_status,
  c.last_bi_direction,
  c.last_bi_mid,
  c.latest_zs_zz,
  c.recent_peak_date,
  c.post_peak_first_bear_date
from stock_stk_factor_pro s
join stock_daily_asof_indicators a
  on a.ts_code = s.ts_code
 and a.trade_date = s.trade_date
 and a.calc_version = 'macd_qfq_12_26_9_v1'
left join stock_daily_rolling_factors r
  on r.ts_code = s.ts_code
 and r.trade_date = s.trade_date
 and r.calc_version = 'rolling_qfq_v1'
left join stock_daily_left_peak l
  on l.ts_code = s.ts_code
 and l.trade_date = s.trade_date
 and l.calc_version = 'left_peak_qfq_v1'
left join stock_daily_czsc_structure c
  on c.ts_code = s.ts_code
 and c.trade_date = s.trade_date
 and c.calc_version = 'czsc_structure_qfq_min6_r7_v1'
where s.trade_date = date '2026-06-26';
```

CZSC 行只在源表存在同一 `(ts_code, trade_date)` 时生成。无效 qfq OHLC 仍有一行，`source_bar_valid=false`、`status=invalid_source_bar`，并沿用此前有效 K 线形成的结构快照。`post_peak_first_bear_missing` 是三态字段：无峰值为 `NULL`，有峰值但尚无阴线为 `true`，已找到阴线为 `false`。

板块/指数示例：

```sql
select
  i.ts_code,
  i.trade_date,
  i.close,
  a.daily_dif_asof,
  a.daily_dea_asof,
  a.weekly_dif_asof,
  a.weekly_dea_asof,
  a.monthly_dif_asof,
  a.monthly_dea_asof
from index_ths_daily i
join index_daily_asof_indicators a
  on a.ts_code = i.ts_code
 and a.trade_date = i.trade_date
 and a.calc_version = 'macd_qfq_12_26_9_v1'
where i.trade_date = date '2026-06-26';
```

`hist_x2` 表示 `2 * (dif - dea)`。如果消费者需要未乘 2 的柱值：

```sql
select weekly_hist_x2_asof / 2.0 as weekly_hist
from stock_daily_asof_indicators
where ts_code = '000001.SZ'
  and trade_date = date '2026-06-26'
  and calc_version = 'macd_qfq_12_26_9_v1';
```

## 为什么不能用昨天 as-of 继续推今天

周线和月线 as-of 表示“上一完成周期状态 + 当日价格推演一次”。在同一个自然周或自然月内，不能用前一交易日的 as-of 继续递推，否则会把同一个未完成周期重复计入。

正确做法：

- EOD 直接读 `*_daily_asof_indicators`。
- 盘中读取上一完成 daily/weekly/monthly 周期状态，再用当前价格递推一次。

错误做法：

```text
今天 weekly_live = 昨天 weekly_asof + 今天价格
```

这个算法在周内第二天开始就会重复使用同一周的未完成价格序列。

## Period State 语义

`stock_period_indicator_state` 和 `index_period_indicator_state` 存的是完成周期状态，不是每日快照。

股票表字段：

```text
ts_code
period_type
period_start_date
period_end_date
period_close_qfq
ema12
ema26
dif
dea
hist_x2
period_count
calc_version
```

指数表字段相同，但收盘字段是 `period_close`，没有复权语义。

`period_type`：

- `daily`: 每个交易日一个完成周期。
- `weekly`: 自然 ISO 周的最后一个交易日形成一个完成周。
- `monthly`: 自然月最后一个交易日形成一个完成月。

读取最近完成周状态：

```sql
select *
from stock_period_indicator_state
where ts_code = '000001.SZ'
  and period_type = 'weekly'
  and period_end_date < date '2026-06-26'
  and calc_version = 'macd_qfq_12_26_9_v1'
order by period_end_date desc
limit 1;
```

## 盘中 MACD 推演

MACD 参数：

```text
alpha12 = 2 / 13
alpha26 = 2 / 27
alpha9 = 2 / 10
```

递推公式：

```text
ema12_live = ema12_prev * (1 - alpha12) + price_live * alpha12
ema26_live = ema26_prev * (1 - alpha26) + price_live * alpha26
dif_live = ema12_live - ema26_live
dea_live = dea_prev * (1 - alpha9) + dif_live * alpha9
hist_x2_live = 2 * (dif_live - dea_live)
```

股票使用盘中前复权价 `price_live_qfq`。如果只有未复权盘中价，需要先按与 `stock_stk_factor_pro.close_qfq` 一致的前复权口径转换，不能直接混用未复权价和 qfq state。

指数和板块使用当前指数价格，对应 `index_period_indicator_state.period_close` 的非复权口径。

### 股票盘中示例

假设 `000001.SZ` 当前前复权价为 `10.23`，推演当前未完成月 MACD：

```sql
with latest_state as (
  select *
  from stock_period_indicator_state
  where ts_code = '000001.SZ'
    and period_type = 'monthly'
    and period_end_date < date '2026-06-26'
    and calc_version = 'macd_qfq_12_26_9_v1'
  order by period_end_date desc
  limit 1
), live_input as (
  select 10.23::double precision as price_live_qfq
), live_calc as (
  select
    s.ts_code,
    s.period_type,
    i.price_live_qfq,
    s.ema12 * (11.0 / 13.0) + i.price_live_qfq * (2.0 / 13.0) as ema12_live,
    s.ema26 * (25.0 / 27.0) + i.price_live_qfq * (2.0 / 27.0) as ema26_live,
    s.dea as dea_prev
  from latest_state s
  cross join live_input i
)
select
  ts_code,
  period_type,
  price_live_qfq,
  ema12_live,
  ema26_live,
  ema12_live - ema26_live as dif_live,
  dea_prev * 0.8 + (ema12_live - ema26_live) * 0.2 as dea_live,
  ((ema12_live - ema26_live) - (dea_prev * 0.8 + (ema12_live - ema26_live) * 0.2)) * 2.0 as hist_x2_live
from live_calc;
```

### 批量股票盘中示例

如果有一张临时表 `live_stock_price(ts_code, price_live_qfq)`：

```sql
with latest_state as (
  select distinct on (p.ts_code, p.period_type)
    p.*
  from stock_period_indicator_state p
  join live_stock_price l on l.ts_code = p.ts_code
  where p.period_type in ('daily', 'weekly', 'monthly')
    and p.period_end_date < date '2026-06-26'
    and p.calc_version = 'macd_qfq_12_26_9_v1'
  order by p.ts_code, p.period_type, p.period_end_date desc
)
select
  s.ts_code,
  s.period_type,
  l.price_live_qfq,
  ema12_live - ema26_live as dif_live,
  s.dea * 0.8 + (ema12_live - ema26_live) * 0.2 as dea_live,
  ((ema12_live - ema26_live) - (s.dea * 0.8 + (ema12_live - ema26_live) * 0.2)) * 2.0 as hist_x2_live
from latest_state s
join live_stock_price l on l.ts_code = s.ts_code
cross join lateral (
  select
    s.ema12 * (11.0 / 13.0) + l.price_live_qfq * (2.0 / 13.0) as ema12_live,
    s.ema26 * (25.0 / 27.0) + l.price_live_qfq * (2.0 / 27.0) as ema26_live
) x;
```

### 指数/板块盘中示例

如果有临时表 `live_index_price(ts_code, price_live)`：

```sql
with latest_state as (
  select distinct on (p.ts_code, p.period_type)
    p.*
  from index_period_indicator_state p
  join live_index_price l on l.ts_code = p.ts_code
  where p.period_type in ('daily', 'weekly', 'monthly')
    and p.period_end_date < date '2026-06-26'
    and p.calc_version = 'macd_qfq_12_26_9_v1'
  order by p.ts_code, p.period_type, p.period_end_date desc
)
select
  s.ts_code,
  s.period_type,
  l.price_live,
  ema12_live - ema26_live as dif_live,
  s.dea * 0.8 + (ema12_live - ema26_live) * 0.2 as dea_live,
  ((ema12_live - ema26_live) - (s.dea * 0.8 + (ema12_live - ema26_live) * 0.2)) * 2.0 as hist_x2_live
from latest_state s
join live_index_price l on l.ts_code = s.ts_code
cross join lateral (
  select
    s.ema12 * (11.0 / 13.0) + l.price_live * (2.0 / 13.0) as ema12_live,
    s.ema26 * (25.0 / 27.0) + l.price_live * (2.0 / 27.0) as ema26_live
) x;
```

## 使用数据库预存长周期指标

日终场景不需要读 period state。直接使用 as-of 表：

```sql
select
  ts_code,
  weekly_dif_asof,
  weekly_dea_asof,
  weekly_hist_x2_asof,
  monthly_dif_asof,
  monthly_dea_asof,
  monthly_hist_x2_asof
from stock_daily_asof_indicators
where trade_date = date '2026-06-26'
  and calc_version = 'macd_qfq_12_26_9_v1';
```

盘中场景需要 period state，因为 as-of 表只代表已落库交易日的 EOD 快照：

```text
latest completed period state + current live price = current live as-of value
```

这套方式避免消费者每次读取三年历史日线，也避免周内/月内重复递推。

## DEA 分位使用

`*_dea_pctile_asof` 是 DEA 对自身历史序列的 inclusive percentile rank，范围是 `0..1`。

示例：

```sql
select
  ts_code,
  weekly_dea_asof,
  weekly_dea_pctile_asof,
  monthly_dea_asof,
  monthly_dea_pctile_asof
from stock_daily_asof_indicators
where trade_date = date '2026-06-26'
  and calc_version = 'macd_qfq_12_26_9_v1'
  and weekly_dea_pctile_asof >= 0.8;
```

早期历史不足时，`*_period_count` 会较小。消费者可以按自己的策略设置最低样本要求：

```sql
where weekly_period_count >= 26
  and monthly_period_count >= 12
```

## Warmup 和覆盖检查

使用 CLI 查询数据库覆盖范围：

```bash
POSTGRES_DSN='postgres://stock_cache:stock_cache@localhost:5432/stock_cache' \
cargo run -- status
```

关注：

- 源表 `min_date/max_date/distinct_dates`。
- 计算表 `min_date/max_date/distinct_dates`。
- `stock_period_indicator_state` 和 `index_period_indicator_state` 的 daily/weekly/monthly 分布。
- `warmup.stock_macd.has_required_start` 和 `warmup.index_macd.has_required_start`。

当 warmup 不足时，长周期 MACD 仍会计算，但早期值只代表当前可用历史内的递推结果，不适合和完整历史口径混用。

## 与 Tushare 自带 MACD 的关系

`stock_stk_factor_pro` 同时保存 Tushare 自带的 `macd_*_bfq/hfq/qfq`。本项目的计算表使用本地 `close_qfq` 递推，版本为 `macd_qfq_12_26_9_v1`。

常规情况下，本地 daily MACD 与 Tushare `macd_*_qfq` 的差异主要来自 Tushare 字段三位小数精度，误差通常在 `0.0005` 附近。复权因子变化或 Tushare 历史修订附近可能出现更大的局部差异。下游应以 `calc_version` 明确选择使用本地口径还是 Tushare 自带口径，不要混用。

## NULL 规则

计算表用 `NULL` 表示缺失、样本不足或不可计算：

- rolling 窗口不足时写 `NULL`。
- 分母为 `0` 或 `NULL` 时，比值写 `NULL`。
- left peak 未定位时，相关日期/价格/布尔值按语义写 `NULL`。

消费者不要把 `NULL` 直接当作 `0`。需要模型输入时，应在特征层显式做缺失处理。
