# Rust CLI Design

本文记录第一版 Rust `stock-cache` CLI 的输入参数、执行边界和运行语义。旧版目录只用于参考 CLI 参数风格，不继承旧版的状态文件、读命令、统计命令、删除命令或安装命令。

## 职责边界

- Rust 负责 CLI、任务编排、PostgreSQL 写入、幂等策略、本地计算因子。
- Python 只作为 Tushare Python SDK 适配层，负责拉取接口数据并把结构化结果交给 Rust。
- Python 不直接写 PostgreSQL。
- CLI 不做分析类工作，不做市场活跃、板块强弱、成分股活跃、模型分、策略筛选或推荐解释。
- CLI 不处理 `broker_recommend`。消费者需要券商推荐信息时自行获取。

## Rust Source Layout

Rust CLI 源码按职责分层，顶层只保留入口、CLI 参数定义和一级模块声明：

```text
src/
  main.rs
  cli.rs

  app/
    mod.rs
    config.rs
    status.rs
    write.rs

  db/
    mod.rs
    init.rs
    write_sql.rs

  source/
    mod.rs
    cache.rs
    writer.rs
    tushare_worker.rs

  metadata/
    mod.rs
    tushare_fields.rs

  calc/
    mod.rs
    constants.rs
    coverage.rs
    source.rs
    model.rs
    writer.rs
    macd.rs
    stock.rs
    index.rs
    rolling.rs
    left_peak.rs
```

模块边界：

- `app/` 放 CLI 命令编排、配置加载和状态查询。
- `db/` 放数据库初始化和静态 SQL 生成，不放 Tushare 拉取流程。
- `source/` 放源数据缓存流程、Python worker 进程适配和源表写入。
- `metadata/` 放固化的 Tushare 字段、写入策略和数据集元信息。
- `calc/` 放本地计算流程；MACD/周期状态、股票计算、指数计算、滚动因子、左侧高点分别拆分。

消费者读取计算表、使用 period state 推演盘中长周期指标时，参见 [Data Usage Guide](data-usage-guide.md)。

## Python Worker Protocol

第一版 Rust 与 Tushare Python SDK 的通信采用常驻 Python worker，不使用 PyO3、gRPC、HTTP 服务、Unix socket 或共享内存。

形态：

```text
Rust parent process
  -> spawn Python worker process
  -> write request JSON lines to worker stdin
  <- read response JSON lines from worker stdout
```

协议约束：

- stdin/stdout 是协议通道，使用 JSON batch messages。
- Python 日志只能写 stderr，不能混入 stdout。
- Python worker 不写数据库。
- Python worker 不改字段名，不转换日期格式，不做业务计算。
- Rust 负责字段校验、类型转换、批量写入、事务和错误处理。
- `tools/discover_tushare_fields.py` 是字段固化阶段的 dev-only 辅助脚本，不属于 Python worker protocol，Rust CLI 运行时不调用它。

请求示例：

```json
{"id":"req-1","dataset":"stock_stk_factor_pro","params":{"trade_date":"20260625"}}
```

响应示例：

```json
{"id":"req-1","type":"batch","rows":[{"ts_code":"000001.SZ","trade_date":"20260625"}]}
{"id":"req-1","type":"end","row_count":5234}
```

错误示例：

```json
{"id":"req-1","type":"error","message":"permission denied","retryable":false}
```

worker 池规则：

- `cache` 阶段启动 Python worker pool。
- worker pool 大小等于当前 `max_concurrency`。
- `calc` 阶段不使用 Python worker。
- `full` 阶段在 `cache` 完成后关闭 worker pool，再进入纯 Rust `calc`。

## Commands

第一版只保留三个命令面：

```bash
stock-cache init-db
stock-cache config show
stock-cache status
stock-cache write --mode full|cache|calc
```

当前实现进度：

- `init-db` 已执行静态 `deploy/postgres/schema.sql` 初始化。
- `config show` 已实现配置加载和脱敏输出。
- `status` 已实现只读数据库覆盖查询，不依赖 Tushare token，不保存 checkpoint/status 表。
- `write --mode cache` 已实现 Tushare Python worker 拉取、Rust 字段校验、事务写入和 9 张源表缓存写入。
- `write --mode calc` 已实现纯本地计算，读取 `stock_stk_factor_pro`、`index_ths_daily` 和 `index_idx_factor_pro`，并写入股票与指数计算表。
- `write --mode full` 已实现 `cache` 成功后执行 `calc` 的编排；`cache` 失败不会进入 `calc`。

不提供：

```text
read
stats
delete
install-skill
```

全局参数：

```bash
stock-cache --env-file /path/to/.env config show
stock-cache --env-file /path/to/.env init-db
stock-cache --env-file /path/to/.env status
stock-cache --env-file /path/to/.env write --mode full
```

`--env-file` 的作用是加载指定环境文件。Shell 环境变量优先级高于 env 文件。

`status` 只需要 `POSTGRES_DSN`。它直接查询当前 PostgreSQL 数据，输出源表、分类表、计算表的行数和日期范围；对 period-state 计算表额外输出 daily/weekly/monthly 分布，并根据计算目标起点反推股票/指数 MACD warmup 覆盖情况。

## Write Modes

`write` 使用一个 `--mode` 控制阶段，不再拆分多个子命令。

| Mode | 含义 | 是否访问 Tushare | 是否计算本地因子 |
| --- | --- | --- | --- |
| `cache` | 拉取 Tushare 源数据，或刷新同花顺分类 | 是 | 否 |
| `calc` | 基于已有 PostgreSQL 源数据计算本地基础因子 | 否 | 是 |
| `full` | `cache` 后执行 `calc` | 是 | 是 |

`calc` 阶段不自动补 Tushare 数据。历史不足时，计算字段写 `NULL` 或结构字段写无效状态，CLI 在 stdout 摘要中给出表级可使用范围。

## Date Window

日期参数保持简单且互斥：

```bash
stock-cache write --mode full --start-date 2026-06-01 --end-date 2026-06-25
stock-cache write --mode full --lookback-trading-days 30
```

规则：

- `--start-date` 和 `--end-date` 必须同时出现。
- `--start-date/--end-date` 与 `--lookback-trading-days` 互斥。
- `calc` 支持基于本地 `stock_stk_factor_pro` 已有交易日的 `--lookback-trading-days` 和默认 `DEFAULT_LOOKBACK_TRADING_DAYS`。
- `full` 支持 `--lookback-trading-days`，会先通过 Tushare `trade_cal` 解析最近 N 个真实交易日，再按这些交易日执行 cache 和 calc。
- 当前 `cache` 单独模式的按日 Tushare 拉取仍要求显式 `--start-date/--end-date`。
- `full` 将用户传入的显式日期区间视为 calc 目标区间，并通过 Tushare `trade_cal` 解析精确交易日。
- `full` 会为 calc 自动补齐目标区间首个交易日前 3 个自然年的 `stock_stk_factor_pro` warmup 源数据；warmup 日期通过 `trade_cal` 精确过滤为交易日，不按自然日或固定交易日数近似。
- warmup 会补齐 `stock_stk_factor_pro`、`index_ths_daily` 和 `index_idx_factor_pro`，供股票和指数长周期计算使用；不拉取资金流或榜单。
- 例外：`write --mode cache --refresh` 且未传任何日期参数时，只刷新当前基础快照，不拉取按日接口。

## Concurrency

保留：

```bash
stock-cache write --mode full \
  --start-date 2026-01-01 \
  --end-date 2026-06-25 \
  --max-concurrency 4
```

`--max-concurrency` 作用于当前 mode 的主要并发任务：

- `cache`: 限制同时处理的交易日或接口任务数量。
- `calc`: 限制同时处理的股票分片或计算任务数量。
- `full`: 两阶段都使用该并发上限。

默认值建议为 `4`，避免过度触发 Tushare 限流。

## Tushare Cache Scope

`cache/full` 的按日数据集固定为：

```text
stock_stk_factor_pro
stock_moneyflow_ths
index_idx_factor_pro
index_ths_daily
index_moneyflow_cnt_ths
index_limit_cpt_list
stock_limit_list_ths
```

各数据集来源：

| 数据集 | 标的来源 | 日期窗口 |
| --- | --- | --- |
| `stock_stk_factor_pro` | Tushare 全市场股票 | 是 |
| `stock_moneyflow_ths` | Tushare 全市场股票 | 是 |
| `index_idx_factor_pro` | `INDEX_LIST_PATH` 中 `enabled=true` 的指数 | 是 |
| `index_ths_daily` | Tushare 日级全量同花顺板块行情 | 是 |
| `index_moneyflow_cnt_ths` | Tushare 日级全量同花顺板块资金流 | 是 |
| `index_limit_cpt_list` | Tushare 日级榜单 | 是 |
| `stock_limit_list_ths` | Tushare 日级榜单 | 是 |

`index_ths_daily` 和 `index_moneyflow_cnt_ths` 按 Python 函数的日级全量形态请求，只传 `trade_date`，不从 `index_ths` 枚举 `ts_code`。

## Refresh Current Snapshots

股票基础列表和同花顺分类基础数据不随历史回填自动刷新。使用显式参数：

```bash
stock-cache write --mode cache --refresh
```

`--refresh` 刷新：

```text
stock_basic
index_ths
index_ths_member
```

刷新语义：

- `stock_basic` 只保存当前股票列表和名称等基础信息。
- `index_ths`/`index_ths_member` 只保存当前板块列表和当前成分关系。
- 不保留历史快照。
- 不按 `trade_date` 每日保存成分。
- 保存 Tushare 文档标注的 `weight/in_date/out_date/is_new` 字段；字段可为空，但应通过 `fields` 参数显式请求。
- 在事务内替换 `stock_basic + index_ths + index_ths_member`，失败保留旧数据。

与日期窗口一起使用时：

```bash
stock-cache write --mode full \
  --start-date 2026-06-01 \
  --end-date 2026-06-25 \
  --refresh
```

执行顺序固定为：

```text
1. refresh
2. cache
3. calc
```

因此 `full --refresh` 会先刷新当前基础快照表，再进入目标日期窗口的数据缓存与计算。

## Index List

`INDEX_LIST_PATH` 只控制 `index_idx_factor_pro`。

CSV 列：

```csv
ts_code,name,category,enabled
000001.SH,上证指数,market,true
399001.SZ,深证成指,market,true
399006.SZ,创业板指,market,true
000300.SH,沪深300,market,true
```

规则：

- `enabled=true` 才拉取。
- `name` 只供人工阅读，不覆盖 Tushare 数据。
- `category` 只供人工分组，不驱动逻辑。
- 不新增 `THS_INDEX_LIST_PATH`。同花顺板块列表来自 `index_ths`。

## Calc Scope

`calc/full` 计算股票级基础因子和同花顺板块指数长周期指标。

股票级基础因子输入来自：

```text
stock_stk_factor_pro
```

统一使用 `stock_stk_factor_pro` 中的前复权字段：

```text
open_qfq
high_qfq
low_qfq
close_qfq
```

以及 `vol/turnover_rate` 等源字段。

指数长周期指标输入来自：

```text
index_ths_daily
index_idx_factor_pro
```

当前写 `index_period_indicator_state` 和 `index_daily_asof_indicators`。前者保存指数 daily/weekly/monthly MACD 完成周期状态，后者保存指数 daily/weekly/monthly MACD as-of 和 DEA 历史分位；不计算市场活跃、板块强弱、成分股活跃或策略筛选分。

## Environment Variables

第一版配置项：

| Variable | Required | Default | Purpose |
| --- | --- | --- | --- |
| `POSTGRES_DSN` | yes | none | PostgreSQL 连接字符串 |
| `TUSHARE_TOKEN` | yes | none | Tushare token |
| `INDEX_LIST_PATH` | no | `config/default-indexes.csv` | `index_idx_factor_pro` 指数清单 |
| `TARGET_CACHE_TABLES_PATH` | no | `config/target-cache-tables.txt` | target cache 阶段要写入的源表清单，一行一个表名，支持 `#` 注释 |
| `DEFAULT_LOOKBACK_TRADING_DAYS` | no | `90` | 默认近期交易日窗口 |
| `MAX_CONCURRENCY` | no | `4` | 默认并发上限 |
| `CACHE_WARMUP_CONCURRENCY` | no | `1` | full 模式补 calc warmup source 时的并发 worker 数；空库大范围 warmup 可调高，注意 Tushare 限流 |
| `REQUEST_TIMEOUT_SECONDS` | no | `30` | Tushare 请求超时 |
| `MAX_RETRIES` | no | `3` | 可重试错误次数 |
| `RETRY_BASE_DELAY_SECONDS` | no | `1.0` | 初始重试延迟 |
| `WRITE_BATCH_SIZE` | no | `1000` | PostgreSQL 批量写入大小 |
| `CALC_CHUNK_SIZE` | no | auto | calc 回补时每个 chunk 的目标交易日数量；未设置时 60 天以内不拆，超过 60 天按 20 天拆 |
| `LOG_LEVEL` | no | `info` | 日志级别 |

不保留：

```text
STATUS_FILE_PATH
ALLOW_INDICATOR_BACKFILL_ON_READ
ENABLE_TUSHARE_INDICATORS
ENABLE_LOCAL_INDICATOR_FALLBACK
```

## Output

数据库不保存运行审计，不建 `job_runs`、`sync_runs`、checkpoint、coverage 或 parity 表。

`write` 完成后在 stdout 输出本次执行摘要 JSON，例如：

```json
{
  "status": "success",
  "mode": "full",
  "calc_date_window": "20260601..20260625",
  "cache_date_window": "20260601..20260625",
  "calc_source_warmup_date_window": "20241210..20260529",
  "source_row_counts": {
    "stock_stk_factor_pro": 123456
  },
  "warmup_source_row_counts": {
    "stock_stk_factor_pro": 98765
  },
  "calc_row_counts": {
    "stock_daily_asof_indicators": 123456
    "index_daily_asof_indicators": 2345
  },
  "usable_ranges": {
    "stock_daily_asof_indicators": ["2026-06-01", "2026-06-25"],
    "index_daily_asof_indicators": ["2026-06-01", "2026-06-25"],
    "stock_daily_rolling_factors": ["2026-06-10", "2026-06-25"],
    "stock_daily_left_peak": [null, "2026-06-25"]
  }
}
```

每次 `write` 或 `status` 成功结束后，摘要 JSON 也会写入：

```text
runtime/database-state.json
```

该文件表示最近一次成功 `write` 或 `status` 后的本地数据库状态快照，用于快速判断当前数据库已经缓存或计算了哪些窗口和行数；运行 `status` 会用数据库当前实际覆盖情况直接覆盖它。它不是数据库内的审计表或 checkpoint 表。

`usable_ranges` 只在输出摘要中出现，不入库。
