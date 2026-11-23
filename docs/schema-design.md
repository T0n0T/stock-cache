# v3 DuckDB 表结构设计

v3 只使用 DuckDB 本地副本。来源表保持 Tushare 接口的一接口一表，计算表统一使用 `calc_` 前缀；字段类型由 `schemas/tushare-fields.yaml` 的 `duckdb_type` 生成到 Rust metadata。

## 计算版本

`calc_version` 描述完整计算契约，包括价格口径、指标参数和结构规则。它不是旧系统兼容字段，也不承担 schema 版本职责。当前默认值为 `v3-macd-kdj-czsc-1`，同一计算表的结果键包含该字段，以便算法调整时并存和回滚。

## 来源表

| 表 | 接口 | 逻辑键 |
| --- | --- | --- |
| `stock_basic` | `stock_basic` | `ts_code` |
| `stock_stk_factor_pro` | `stk_factor_pro` | `trade_date, ts_code` |
| `stock_moneyflow` | `moneyflow` | `trade_date, ts_code` |
| `index_idx_factor_pro` | `index_idx_factor_pro` | `trade_date, ts_code` |
| `index_weight` | `index_weight` | `index_code, trade_date, con_code` |
| `index_ths` | `ths_index` | `ts_code` |
| `index_ths_daily` | `index_ths_daily` | `trade_date, ts_code` |
| `index_ths_member` | `ths_member` | `ts_code, con_code` |
| `index_sw_classify` | `index_classify` | `index_code` |
| `index_sw_member` | `index_member_all` | `ts_code, in_date` |
| `index_limit_cpt_list` | `limit_cpt_list` | `trade_date, ts_code` |
| `stock_limit_list_ths` | `limit_list_ths` | `trade_date, ts_code` |

`index_weight` 按月保存配置中 `category=market` 的市场指数成分权重，字段为 `index_code`, `trade_date`, `con_code`, `weight`，键为 `index_code, trade_date, con_code`。`--refresh` 会按指数、按年分块抓取 2020 年以来的月末快照，避免触发 Tushare 的 10 万行 offset 上限。它只用于成分归因和回测，不参与指标计算。

## 计算表

### `calc_period_bars`

已完成的周/月 OHLC，供 MACD、KDJ、CZSC 共用。字段为 `source_dataset`, `ts_code`, `period_type`, `period_start_date`, `period_end_date`, `price_basis`, `open`, `high`, `low`, `close`, `vol`, `amount`, `calc_version`；键为来源、标的、周期和结束日加版本。

### `calc_indicator_state`

已完成周期的递推检查点。保存 `macd_ema12`, `macd_ema26`, `macd_dea`, `macd_period_count`, `kdj_k`, `kdj_d`, `kdj_period_count` 及来源、周期、日期和版本。`dif`、柱值和 KDJ J 值由状态派生。

### `calc_daily_indicators`

每个来源标的和交易日一行，保存 daily/weekly/monthly MACD as-of、DEA 分位、样本数，以及 weekly/monthly KDJ K/D/J。键为 `source_dataset, ts_code, trade_date, calc_version`。

### `calc_daily_czsc_structure`

每个来源标的和交易日一行，按 `daily_`、`weekly_`、`monthly_` 保存完成标志、有效性、笔/中枢、近期峰值和峰值后首根阴线字段。周/月未完成时只输出截至当日可见结构。

### `calc_daily_rolling_factors` 与 `calc_daily_left_peak`

保存日线 rolling 和左峰结构结果，键同样包含来源、标的、交易日和版本。窗口不足、无效 OHLC、分母为零和无法定位结构时写 `NULL`。

### `calc_daily_industry_moneyflow`

本地派生的申万 2021 二级行业资金流，不对应 Tushare 接口。按 `stock_moneyflow` + `index_sw_member` 的时点成分聚合，字段包括 `industry_code`, `industry_name`, `trade_date`, `stock_count`，小/中/大/特大单的买卖额与净额，`main_net_amount`（大单+特大单净额）、`net_mf_vol`、`net_mf_amount` 和 `calc_version`。键为 `source_dataset, industry_code, trade_date, calc_version`，当前版本为 `sw2021-l2-v1`。

聚合时用 `in_date <= trade_date AND (out_date IS NULL OR trade_date < out_date)` 选择时点有效成分；同一股票同一天若匹配多条记录，取 `in_date` 最新的一条。未被申万成分覆盖的股票不进入行业合计，因此行业净额之和等于当天所有被映射股票的净额之和。该表按日期窗口幂等重建，`--mode cache/full/calc` 都会在抓取或计算阶段刷新目标窗口；未取到 `stock_moneyflow` 时跳过，有源数据但 `index_sw_member` 为空时报错并提示先执行 `--refresh`。

## 物理和写入设计

所有来源字段使用显式 DuckDB 类型：日期 `DATE`，时间戳 `TIMESTAMP`，整数 `BIGINT`，数值 `DOUBLE`，标识和枚举 `VARCHAR`，布尔标志 `BOOLEAN`。Rust 初始化代码为每个表建立逻辑主键；大批量写入使用 staging + `INSERT ... SELECT`，不复制逐行数据库客户端逻辑。

- 每日追加：一个批次、一个事务、`append`。
- 过去一段时间补全：按日期分块、每块一次 `replace-dates`。
- 单日替换或删改：单日期 `replace-dates`；精确删除使用 `replace-keys`。
- 计算回补：输入带 warmup，先根据 checkpoint 规划失效范围，再整体替换结果日期。

## 性能指标口径

主指标是从输入交给 Rust writer 到提交和 checkpoint 完成的 wall time，报告逻辑行数、输入 MiB、rows/s、中位数和 P95。另行报告从 API 请求开始到 DuckDB 提交完成的端到端时间，并拆分 HTTP/解码、JSONL 序列化、数据库写入。端到端计时不包含 MACD/KDJ/CZSC 计算；计算批次另行从源读取开始计时到计算结果提交结束。

最近一周真实拉取（2026-11-09 至 2026-11-13）为 27,864 fetched rows、27,452 written logical rows：HTTP/解码中位 4,521.7 ms，序列化 5,696.2 ms，DuckDB 写入与 checkpoint 1,059.4 ms，命令内部总计 11,378.3 ms，含进程启动 wall time 11,592.6 ms。对应端到端中位吞吐约 489.9 fetched rows/s，writer 约 5,260.4 written rows/s。

## 缓存调度辅助表

| 表 | 字段 | 用途 |
|---|---|---|
| `cache_trade_calendar` | `trade_date DATE PRIMARY KEY` | SSE 开市日；包含用于判断周/月是否完成的未来交易日 |
| `cache_fetch_coverage` | `dataset VARCHAR, trade_date DATE, code VARCHAR, row_count BIGINT` | 主键为前三列；记录每次成功日期请求的实际去重写入行数，指数按代码独立记录 |

空响应的 `row_count=0` 保留请求覆盖；股票行情为空时不记录成功。复用历史覆盖前
会与实际来源表行数核对，删改来源导致计数变化时，full 会重新补齐，calc 则拒绝
以不完整历史计算。两表由 DuckDB schema 初始化统一建立，不属于六张因子计算表。
