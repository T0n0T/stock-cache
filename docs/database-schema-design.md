# Database Schema Design

本文记录第一版 PostgreSQL schema 设计。数据库只保存 Tushare 源事实、当前同花顺分类关系、股票级本地基础计算结果和同花顺板块指数长周期指标。不保存运行审计、分析聚合或消费者视图。

## Initialization

第一版假设目标数据库为空。

- `init-db` 执行固定 schema 初始化。
- 不设计旧 schema 升级迁移。
- 不建 `schema_migrations`。
- 如果目标库存在同名表且结构冲突，`init-db` 应失败并提示使用空库或手动处理。
- `init-db` 不执行破坏性删除。

## Global Rules

源表规则：

- 每一列对应 Tushare 返回字段。
- 不加本地元数据列。
- 不使用 `raw_jsonb` 或 `extra_jsonb` 兜底。
- 不加 `fetched_at`、`ingest_run_id`、`updated_at`。
- 字段名尽量沿用 Tushare 原名。

字段固化流程：

1. 实现前先查官方 Tushare 文档，确认接口名、参数、返回字段和含义。
2. 用 Tushare SDK 对每个接口拉样本，确认实际返回字段。
3. 将字段名、PostgreSQL 类型、主键、写入策略和拉取计划记录到 `schemas/tushare-fields.yaml`。
4. 从 YAML 生成 `docs/tushare-fields.md`、`deploy/postgres/schema.sql`、Rust row struct 和静态 insert/upsert SQL 草稿。
5. 人工审查生成结果后提交为静态代码。
6. 运行时如果遇到 schema 未声明的未知字段，直接报错，不静默忽略。

字段 source of truth：

```text
schemas/tushare-fields.yaml
```

已生成的人读版字段字典：

```text
docs/tushare-fields.md
```

已生成的静态初始化 SQL：

```text
deploy/postgres/schema.sql
```

`deploy/postgres/schema.sql` 由 `schemas/tushare-fields.yaml` 生成并提交为静态文件。Rust `init-db` 运行时只执行该固定 SQL，不读取 YAML、不动态建列。

第一版 `schemas/tushare-fields.yaml` 固化官方输出参数中的默认字段和可选字段。SDK 默认样本未返回但官方文档列出的可选字段也应入库，并在拉取时通过 Tushare `fields` 参数显式请求；如果显式请求仍不可用，再写入对应 dataset 的 `doc_mismatch`。

规则：

- dataset id 必须等于数据库表名。
- Tushare API 名用 `api` 字段单独记录。
- 每个字段显式记录 `pg_type`、`nullable`、`description`。
- pandas dtype 或样本值只能作为建议，不能自动决定 PostgreSQL 类型。
- 每个 dataset 记录 `write_strategy`，例如 `upsert` 或 `delete_insert_by_date`。
- 每个 dataset 记录 `fetch_plan`，例如 `by_trade_date`、`by_trade_date_and_code_list`、`refresh_current`。
- `docs/tushare-fields.md` 全部由 YAML 生成，不手工修改。
- 人工备注、官方文档差异、接口坑也写回 YAML。

示例：

```yaml
datasets:
  stock_stk_factor_pro:
    api: stk_factor_pro
    doc_url: https://tushare.pro/document/2?doc_id=328
    checked_at: 2026-06-26
    sdk_version: 1.4.29
    write_strategy:
      type: upsert
      key: [trade_date, ts_code]
    fetch_plan:
      kind: by_trade_date
      params: [trade_date]
    fields:
      - name: ts_code
        pg_type: text
        nullable: false
        description: TS 股票代码
      - name: trade_date
        pg_type: date
        nullable: false
        source_format: YYYYMMDD
        description: 交易日期
      - name: close_qfq
        pg_type: double precision
        nullable: true
        description: 前复权收盘价
```

官方文档和 SDK 实际返回不一致时：

- 以实际 SDK 返回字段为准。
- 在 `schemas/tushare-fields.yaml` 标注文档差异，并生成到 `docs/tushare-fields.md`。
- 不保存官方 HTML，只记录文档链接、确认日期、SDK 版本和字段清单。

允许纳入 dev-only 字段发现和代码生成脚本，例如：

```text
tools/discover_tushare_fields.py
tools/generate_schema.py
```

这些脚本只用于实现前 schema 辅助和未来手动检查 Tushare 字段变化。生成结果必须提交为静态代码并经过人工审查。运行时 Rust CLI 不使用这些脚本，不反射写库，不动态建列。

计算表规则：

- 保留 `calc_version`，它是计算口径的一部分。
- 不加 `updated_at`。
- 重跑同一 `calc_version` 时按主键 upsert 覆盖。
- 新算法或新口径写新的 `calc_version`。

不建：

```text
job_runs
sync_runs
status/checkpoint 表
coverage/report/parity 表
market_daily_activity
ths_sector_daily_strength
ths_sector_daily_member_stats
SQL view
broker_recommend 相关表
```

## Source Tables

第一版源表：

```text
stock_stk_factor_pro
stock_moneyflow_ths
index_idx_factor_pro
index_ths
index_ths_daily
index_moneyflow_cnt_ths
index_ths_member
index_limit_cpt_list
stock_limit_list_ths
```

源表字段需要在实现时用实际 Tushare 返回字段确认。下面只记录主键和写入语义。

### stock_stk_factor_pro

用途：个股前复权行情、估值和技术指标源表。

推荐主键：

```sql
primary key (trade_date, ts_code)
```

本地计算层唯一股票行情输入来自此表。已确认它包含大量 qfq 字段，例如：

```text
open_qfq
high_qfq
low_qfq
close_qfq
ma_qfq_5/10/20/30/60/90/250
macd_qfq/macd_dif_qfq/macd_dea_qfq
kdj_k_qfq/kdj_d_qfq/kdj_qfq
boll_*_qfq
bias*_qfq
roc_qfq
mtm_qfq
psy_qfq
wr_qfq
rsi_qfq_6/12/24
```

### stock_moneyflow_ths

用途：个股同花顺资金流源表。

推荐主键：

```sql
primary key (trade_date, ts_code)
```

只落源字段，不做资金强度派生。

### index_idx_factor_pro

用途：预设指数因子行情源表。

标的来自 `INDEX_LIST_PATH` 中 `enabled=true` 的 `ts_code`。

推荐主键：

```sql
primary key (trade_date, ts_code)
```

### index_ths

用途：同花顺板块/指数基础列表。

刷新方式：

- 由 `write --mode cache --refresh-ths` 或 `full --refresh-ths` 触发。
- 只保存当前列表。
- 与 `index_ths_member` 在同一事务内整表替换。
- 失败保留旧数据。

主键以实际接口字段为准，通常为同花顺板块/指数代码字段。

### index_ths_member

用途：当前同花顺板块成分关系。

规则：

- 只保存当前关系。
- 不保留历史快照。
- 不保存每日 `trade_date` 快照。
- 保存 Tushare 文档标注的 `weight/in_date/out_date/is_new` 字段；即使当前样本中 `weight/in_date/out_date` 全为空，也按官方输出参数保留为 nullable 字段。
- 直接作为消费者查询股票和板块关系的入口，不提供 SQL view。

推荐主键：

```sql
primary key (ts_code, con_code)
```

推荐索引：

```sql
create index on index_ths_member (con_code);
create index on index_ths_member (ths_code);
```

### index_ths_daily

用途：同花顺板块/指数日行情源表。

数据来源：Tushare `index_ths_daily` 日级全量返回。缓存请求只传 `trade_date`，不枚举
`index_ths.ts_code`。

推荐主键：

```sql
primary key (trade_date, ts_code)
```

如果实际接口代码字段不是 `ts_code`，按实际字段调整。

### index_moneyflow_cnt_ths

用途：同花顺概念/板块资金流源表。

数据来源：Tushare `index_moneyflow_cnt_ths` 日级全量返回。缓存请求只传 `trade_date`。

推荐主键以实际接口字段为准，优先使用：

```sql
primary key (trade_date, ts_code)
```

只落源字段，不做资金强度派生。

### index_limit_cpt_list

用途：同花顺最强板块/概念榜源表。

写入策略：

- 如果实现时确认存在稳定唯一键，使用业务主键 upsert。
- 否则按 `trade_date` delete + insert。

不在缓存层与 `index_ths` 建外键或关系表。消费者可按 Tushare 返回代码或名称自行关联。

### stock_limit_list_ths

用途：同花顺涨跌停榜源表。

推荐主键：

```sql
primary key (trade_date, ts_code)
```

只落源字段，不做市场活跃或板块活跃统计。

## Source Write Strategy

重复执行同一个日期窗口时：

- 有稳定业务主键的源表使用 upsert。
- 没有稳定业务主键的源表按日期切片 delete + insert。
- `index_ths + index_ths_member` 使用事务化整表替换。

## Calculation Tables

### stock_period_indicator_state

用途：保存每只股票 daily/weekly/monthly MACD 12/26/9 的完成周期递推状态。消费者可用上一完成周期状态和盘中前复权价自行计算 live MACD。

字段：

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

不包含：

```text
period_key
finalized
hist
updated_at
```

约束：

```sql
check (period_type in ('daily', 'weekly', 'monthly'))
primary key (ts_code, period_type, period_end_date, calc_version)
```

推荐索引：

```sql
create index on stock_period_indicator_state (ts_code, period_type, period_end_date desc);
```

### stock_daily_asof_indicators

用途：保存每个交易日的 daily/weekly/monthly MACD as-of 快照和 DEA 自身历史分位。

字段：

```text
ts_code
trade_date

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

calc_version
```

不保存 `hist`。如消费者需要 `hist = dif - dea`，可用 `hist_x2 / 2`。

主键：

```sql
primary key (ts_code, trade_date, calc_version)
```

### index_daily_asof_indicators

用途：保存同花顺板块/指数每个交易日的 daily/weekly/monthly MACD as-of 快照和 DEA 自身历史分位。

数据来源：`index_ths_daily` 的 `open/high/low/close` 日行情。当前版本只保存 as-of 长周期指标，不保存板块指数 rolling/left-peak 股票专用结构。

### index_period_indicator_state

用途：保存指数 daily/weekly/monthly MACD 12/26/9 的完成周期递推状态，供盘中用当前指数价格推演 live 周/月 MACD。

数据来源包括：

```text
index_ths_daily
index_idx_factor_pro
```

其中 `index_ths_daily` 覆盖同花顺板块指数，`index_idx_factor_pro` 覆盖 `config/default-indexes.csv` 启用的默认市场指数。

字段与 `stock_period_indicator_state` 对齐，但收盘字段命名为 `period_close`，不使用复权语义。

主键：

```sql
primary key (ts_code, period_type, period_end_date, calc_version)
```

推荐索引：

```sql
create index on index_period_indicator_state (ts_code, period_type, period_end_date desc);
```

字段与 `stock_daily_asof_indicators` 相同：

```text
ts_code
trade_date

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

calc_version
```

主键：

```sql
primary key (ts_code, trade_date, calc_version)
```

### stock_daily_rolling_factors

用途：补充 `stock_stk_factor_pro` 没有直接提供的基础窗口/位置/量价滚动字段。

字段：

```text
ts_code
trade_date

ma25_qfq
ma144_qfq
ma220_qfq

high_20_qfq
high_90_qfq
low_90_qfq
high_120_qfq
low_120_qfq
close_to_20d_max_close_pct
position_90d
position_120d

volume_ma5
volume_ma20
volume_to_ma5_ratio
volume_to_ma20_ratio
volume_ma5_to_ma20_ratio

turnover_rate_ma5
turnover_to_ma5_ratio

range_compression_20d
range_compression_40d

calc_version
```

不重复保存 `stock_stk_factor_pro` 已有 qfq 指标，例如 KDJ、日线 MACD、BOLL、BIAS、ROC、MTM、PSY、WR、RSI、CCI、OBV、VR、TRIX、常见 MA/EMA。

主键：

```sql
primary key (ts_code, trade_date, calc_version)
```

### stock_daily_left_peak

用途：保存本地计算版本文档定义的左侧压力位、突破结构和第一阴线锚点。只做 EOD，不做 intraday live。

字段：

```text
ts_code
trade_date
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
calc_version
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

布尔字段不可适用时写 `NULL`，不写 `false` 伪装。例如左峰未定位时，`first_bear_missing` 写 `NULL`。

主键：

```sql
primary key (ts_code, trade_date, calc_version)
```
