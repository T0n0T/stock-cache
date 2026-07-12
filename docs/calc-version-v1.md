# Calculation Versions V1

本文记录第一版本地计算表的计算口径。所有本地计算只使用 `stock_stk_factor_pro`，并统一使用前复权价格字段。

## Version Names

各计算表独立使用 `calc_version`：

| 表 | calc_version |
| --- | --- |
| `stock_period_indicator_state` | `macd_qfq_12_26_9_v1` |
| `stock_daily_asof_indicators` | `macd_qfq_12_26_9_v1` |
| `stock_daily_rolling_factors` | `rolling_qfq_v1` |
| `stock_daily_left_peak` | `left_peak_qfq_v1` |
| `stock_daily_czsc_structure` | `czsc_structure_qfq_min6_r7_v1` |

`calc_version` 是计算口径的一部分，不是运行审计字段。

## Shared Input Rules

股票级本地计算唯一输入表：

```text
stock_stk_factor_pro
```

价格字段统一使用：

```text
open_qfq
high_qfq
low_qfq
close_qfq
```

量能和换手使用源字段：

```text
vol
turnover_rate
```

缺失值规则：

- 输入缺失时输出 `NULL`。
- 窗口样本不足时输出 `NULL`。
- 分母为 `0` 或 `NULL` 时，比值输出 `NULL`。
- 不用 `0` 伪装缺失或不可计算。

## Source Indicator Contract

`stock_stk_factor_pro` 中的 Tushare 技术指标按源字段保存，不在缓存层改名。

KDJ 字段定义：

| 字段 | 含义 |
| --- | --- |
| `kdj_k_qfq` | 前复权 K 值 |
| `kdj_d_qfq` | 前复权 D 值 |
| `kdj_qfq` | 前复权 J 值 |
| `kdj_k_hfq/kdj_d_hfq/kdj_hfq` | 后复权 K/D/J |
| `kdj_k_bfq/kdj_d_bfq/kdj_bfq` | 不复权 K/D/J |

发布契约要求 `kdj_qfq` 在容差内满足：

```text
kdj_qfq = 3 * kdj_k_qfq - 2 * kdj_d_qfq
```

缓存层不输出 `kdj_j_qfq/kdj_j_hfq/kdj_j_bfq` 物理列。
消费者如需要 J 字段名，应使用 `kdj_qfq AS kdj_j_qfq`，或按上式派生。

## MACD State

版本：`macd_qfq_12_26_9_v1`。

参数：

```text
fast = 12
slow = 26
signal = 9
alpha12 = 2 / (12 + 1)
alpha26 = 2 / (26 + 1)
alpha9 = 2 / (9 + 1)
```

EMA 初始化：首个有效值以自身作为种子。

递推公式：

```text
ema12 = alpha12 * close + (1 - alpha12) * ema12_prev
ema26 = alpha26 * close + (1 - alpha26) * ema26_prev
dif = ema12 - ema26
dea = alpha9 * dif + (1 - alpha9) * dea_prev
hist_x2 = 2 * (dif - dea)
```

不保存 `hist = dif - dea`。消费者如需要可用：

```text
hist = hist_x2 / 2
```

### Period Types

`stock_period_indicator_state.period_type` 支持：

```text
daily
weekly
monthly
```

周期输入：

| period_type | 输入 close | 周期说明 |
| --- | --- | --- |
| `daily` | 每个完成交易日 `close_qfq` | 每个交易日一个完成周期 |
| `weekly` | 自然 ISO 周最后一个交易日 `close_qfq` | 每个完成自然周一个周期 |
| `monthly` | 自然月最后一个交易日 `close_qfq` | 每个完成自然月一个周期 |

`period_start_date` 和 `period_end_date` 保存周期范围。主键使用 `period_end_date`，不使用派生字符串 `period_key`。

weekly/monthly state 只在自然周或自然月完成后落表。
因此 pick date 不是周末/月末时，消费者应取 `period_end_date <= pick_date` 的最新状态。

极短历史股票可能尚未形成 weekly 或 monthly state。
这类缺口不是计算错误，但必须在发布前 coverage report 中列出，供下游跳过。

### As-Of Daily Values

`stock_daily_asof_indicators` 保存 daily/weekly/monthly 的日级 as-of 快照：

```text
daily_dif_asof
daily_dea_asof
daily_hist_x2_asof
daily_dea_pctile_asof
daily_period_count

weekly_dif_asof
weekly_dea_asof
weekly_hist_x2_asof
weekly_dea_pctile_asof
weekly_period_count

monthly_dif_asof
monthly_dea_asof
monthly_hist_x2_asof
monthly_dea_pctile_asof
monthly_period_count
```

口径：

- `daily_*_asof`: 截至 `trade_date` 的本地日线 MACD 递推结果。
- `weekly_*_asof`: 上一完成周状态加当日 `close_qfq` 得到的当前未完成周 as-of。
- `monthly_*_asof`: 上一完成月状态加当日 `close_qfq` 得到的当前未完成月 as-of。

周内或月内不能用前一日 as-of 继续递推，否则会把同一个未完成周期重复计入。

### DEA Percentile

DEA 分位使用 inclusive percentile rank：

```text
dea_pctile = count(dea <= current_dea) / count(dea)
```

样本集合：

- `daily_dea_pctile_asof`: 该股票截至 `trade_date` 的 daily DEA 状态序列，包含当日 `daily_dea_asof`。
- `weekly_dea_pctile_asof`: 当前周开始日期之前的完成 weekly DEA 序列，加当前 `weekly_dea_asof`。
- `monthly_dea_pctile_asof`: 当前月开始日期之前的完成 monthly DEA 序列，加当前 `monthly_dea_asof`。

这样不会把同一个未完成周或未完成月在多个交易日的 as-of 值重复计入历史分布。

### Intraday Consumer Formula

本 CLI 不写 live 表，也不提供 SQL function。消费者如需盘中 daily/weekly/monthly MACD，可读取上一完成周期状态，再用盘中前复权价递推一次：

```text
ema12_live = alpha12 * price_live_qfq + (1 - alpha12) * ema12_prev
ema26_live = alpha26 * price_live_qfq + (1 - alpha26) * ema26_prev
dif_live = ema12_live - ema26_live
dea_live = alpha9 * dif_live + (1 - alpha9) * dea_prev
hist_x2_live = 2 * (dif_live - dea_live)
```

## Rolling Factors

版本：`rolling_qfq_v1`。

### Moving Averages

```text
ma25_qfq = avg(close_qfq, last 25 trading days)
ma144_qfq = avg(close_qfq, last 144 trading days)
ma220_qfq = avg(close_qfq, last 220 trading days)
```

窗口不足写 `NULL`。

### Price Ranges And Position

```text
high_20_qfq = max(high_qfq, last 20 trading days)
high_90_qfq = max(high_qfq, last 90 trading days)
low_90_qfq = min(low_qfq, last 90 trading days)
high_120_qfq = max(high_qfq, last 120 trading days)
low_120_qfq = min(low_qfq, last 120 trading days)
```

近 20 日最高收盘位置：

```text
close_to_20d_max_close_pct = close_qfq / max(close_qfq, last 20 trading days) - 1
```

区间位置：

```text
position_Nd = (close_qfq - rolling_low_qfq_N) / (rolling_high_qfq_N - rolling_low_qfq_N)
```

其中 N 为 `90` 或 `120`。分母为 `0` 时写 `NULL`。

### Volume Factors

```text
volume_ma5 = avg(vol, last 5 trading days)
volume_ma20 = avg(vol, last 20 trading days)
volume_to_ma5_ratio = vol / volume_ma5
volume_to_ma20_ratio = vol / volume_ma20
volume_ma5_to_ma20_ratio = volume_ma5 / volume_ma20
```

分母为 `0` 或 `NULL` 时写 `NULL`。

### Turnover Factors

```text
turnover_rate_ma5 = avg(turnover_rate, last 5 trading days)
turnover_to_ma5_ratio = turnover_rate / turnover_rate_ma5
```

不保留 `turnover_n_43d`。

### Range Compression

```text
range_compression_Nd = (rolling_high_qfq_N - rolling_low_qfq_N) / close_qfq
```

其中 N 为 `20` 或 `40`：

```text
range_compression_20d = (max(high_qfq, 20) - min(low_qfq, 20)) / close_qfq
range_compression_40d = (max(high_qfq, 40) - min(low_qfq, 40)) / close_qfq
```

值越小，表示最近 N 日价格区间越窄。

## Left Peak

版本：`left_peak_qfq_v1`。

算法口径以本节和 Rust 实现 `src/calc/left_peak.rs` 为准。

输入：单只股票截至 `trade_date` 的前复权日线：

```text
trade_date
open_qfq
high_qfq
low_qfq
close_qfq
```

默认参数：

| 参数 | 默认值 | 含义 |
| --- | --- | --- |
| `swing_lookback` | `2` | 摆动高点判定左右各看的根数 |
| `breakout_lookback` | `5` | 最近 N 根内检测突破 |
| `high_cycle_days` | `360` | 选左峰时只看近 360 个交易日内候选峰 |
| `min_pullback_pct` | `5.0` | 左峰到突破前最低回调幅度阈值 |

输出字段：

```text
left_peak_date
left_peak_high
breakout_date
breakout_close
breakout_body_above_left_peak
first_bear_date
first_bear_open
first_bear_missing
pick_close
b_div_a
abs_ba_minus_1
a_lt_b
is_valid
status
```

`status` 取值：

```text
insufficient_history
no_swing_high
no_cycle_peak
no_close_breakout
no_body_breakout
pullback_too_small
valid
```

`is_valid=true` 仅当突破验证全部通过。

第一阴线锚点规则：

- 从左峰日期之后开始扫描，到 `trade_date` 为止。
- 阴线定义为 `close_qfq < open_qfq`。
- 取第一根阴线的日期和开盘价，写入 `first_bear_date/first_bear_open`。
- 左峰已定位但没有阴线时，`first_bear_missing=true`。
- 左峰未定位时，`first_bear_missing=NULL`。
- 只要左峰已定位，即使 `is_valid=false`，也尽量填充 `first_bear_*`。

`B/A` 字段：

```text
pick_close = trade_date 当日 close_qfq，记为 B
first_bear_open = 第一阴线 open_qfq，记为 A
b_div_a = B / A
abs_ba_minus_1 = abs(B / A - 1)
a_lt_b = A < B
```

`A` 缺失时，上述派生字段写 `NULL`。

## CZSC Structure

版本：`czsc_structure_qfq_min6_r7_v1`。

输入仅使用按股票、交易日严格递增的 `open_qfq/high_qfq/low_qfq/close_qfq`。停牌日期不补 K 线；无效 OHLC 不送入分析器，但仍输出沿用既有状态的当日行。核心参数为：

```text
min_bi_len = 6
max_bi_num = 50
recent_bi_window = 7
```

所有派生结构基于 `finished_bis`，并严格在目标交易日当场生成，未来笔延伸、破坏或撤销不会回写过去快照。输出包括最近完成笔及其中点、最后一个有效 ZS、最近最多 7 笔覆盖原始 K 线的峰值，以及峰值之后第一根 `close_qfq < open_qfq` 的阴线。并列最高价取最后一个实际交易日。

`status` 按以下优先级互斥：

```text
invalid_source_bar
no_finished_bi
insufficient_bis_for_zs
no_valid_zs
ok
```

结构算法、完成笔尾部判定、参数、ZS 划分、recent 窗口、峰值并列规则、阴线定义或 as-of 语义发生变化时必须升级 `calc_version`。仅源行情修订不升级版本，使用同版本 UPSERT 覆盖。

## Usable Ranges

`write --mode calc/full` 完成后，stdout 摘要输出每张计算表的保守可使用范围，不入库。

定义：

- `stock_daily_asof_indicators`: 输出实际请求范围。消费者可参考 `*_period_count` 判断早期 warmup。
- `stock_daily_rolling_factors`: 最大窗口为 220 个交易日，因此可使用起点为本地可用历史中的第 220 个交易日。
- `stock_daily_left_peak`: 保守窗口为 360 个交易日，因此可使用起点为本地可用历史中的第 360 个交易日。
- `stock_daily_czsc_structure`: 没有固定 N 日预热期，但有路径依赖；输出范围是实际请求范围，计算必须从每只股票最早本地历史恢复状态。

如果本地历史不足对应窗口，起点输出 `null`。

## DB-native Release Gate

`contract-check stock-select` 是面向 `stock-select` 发布前 smoke 的只读门禁。

建议命令：

```bash
cargo run -- contract-check stock-select \
  --pick-date 20260615 \
  --lookback-trading-days 252 \
  --period-coverage-threshold 0.995
```

检查范围：

- 最新交易日 `stock_stk_factor_pro` 行数大于 0。
- 近 252 个交易日窗口存在且 `adj_factor` 全量非空、有限、`> 0`。
- 最新交易日 qfq OHLC 非空覆盖完整。
- KDJ 满足 K/D/J 契约，且 `kdj_qfq` 是 J 值。
- 三张股票计算表按指定 `calc_version` 与股票 universe 对齐。
- `stock_period_indicator_state` 输出三周期覆盖率和缺失列表。
- THS 行情、MACD as-of、资金流和 membership 表可用。

阻断项以 `blocking_failures` 输出。
短历史股票缺 weekly/monthly state 时，缺失记录应标记为可接受跳过。
