# Stock Cache

`stock-cache` 是一个基于 Typer 的 CLI 工具，用于从 Tushare 拉取近期 A 股市场数据，将规范化后的行数据存入 PostgreSQL，并提供以 JSON 形式读取原始历史数据和执行简单筛选查询的命令。

## 项目功能

- 从 Tushare 获取股票列表和最近的交易日
- 按交易日批量同步日行情、基础数据、资金流向、复权、停牌、涨跌停、指标和筹码分布/筹码表现等数据
- 从可配置的默认指数清单中同步主要指数、主题指数和申万二级行业指数日线
- 将规范化后的行数据写入 PostgreSQL
- 记录任务运行摘要，并为最近一次写入任务写出固定状态文件
- 以 JSON 形式读取缓存数据，供下游脚本或人工检查使用

## 运行要求

- Python `3.13`
- `uv`
- PostgreSQL
- 有效的 `TUSHARE_TOKEN`

仓库内已在 [compose.yml](compose.yml) 中提供本地 PostgreSQL 服务定义。

## 快速开始

1. 安装依赖：

```bash
uv sync
```

2. 启动 PostgreSQL。可以使用仓库自带的 compose 文件：

```bash
docker compose up -d postgres
```

3. 创建环境变量文件：

```bash
cp .env.example .env
```

4. 至少在 `.env` 中配置：

- `POSTGRES_DSN`
- `TUSHARE_TOKEN`

5. 初始化数据库 schema：

```bash
uv run stock-cache init-db
```

6. 执行首次写入：

```bash
uv run stock-cache write --mode full
```

如果要查看当前 shell 或 env 文件下最终生效的运行时配置值，可执行：

```bash
uv run stock-cache config show
uv run stock-cache --env-file /path/to/.env config show
```

## 运行 CLI

支持的入口形式如下：

- `uv run stock-cache ...`
- `uv run python -m cli ...`

下面的示例默认使用控制台脚本形式。

如果要强制 CLI 读取指定 env 文件，请在子命令前传入全局参数：

```bash
uv run stock-cache --env-file /path/to/.env write --mode full
```

通过 shell `export` 设置的环境变量仍然会覆盖 `--env-file` 或默认 `.env` 中加载的值。

## 全局安装与独立技能

从当前代码检出目录安装该工具及其独立技能：

```bash
uv run stock-cache install-skill --token YOUR_TUSHARE_TOKEN
```

安装完成后，支持以下两种运行方式：

```bash
stock-cache --help
uv tool run stock-cache --help
```

安装后的独立目录为：

```text
~/.agents/skills/stock-cache
```

安装后可直接编辑的默认指数清单位于：

```text
~/.agents/skills/stock-cache/.runtime/default-indexes.csv
```

## 环境变量

| Variable | Required | Default | Purpose |
| --- | --- | --- | --- |
| `POSTGRES_DSN` | 是 | 无 | PostgreSQL 连接字符串 |
| `TUSHARE_TOKEN` | 是 | 无 | Tushare API token |
| `MAX_CONCURRENCY` | 否 | `20` | 预留的并发设置 |
| `MAX_RETRIES` | 否 | `3` | 可重试 provider 失败时的重试次数 |
| `RETRY_BASE_DELAY` | 否 | `1.0` | 初始重试延迟（秒） |
| `RETRY_BACKOFF_FACTOR` | 否 | `2.0` | 指数退避倍数 |
| `RETRY_JITTER` | 否 | `0.2` | 随机重试抖动（秒） |
| `REQUEST_TIMEOUT_SECONDS` | 否 | `20` | provider 请求超时时间 |
| `DEFAULT_LOOKBACK_TRADING_DAYS` | 否 | `90` | 默认写入窗口覆盖的近期交易日数量 |
| `STATUS_FILE_PATH` | 否 | `.runtime/last-write-status.txt` | 最近一次写入任务的摘要状态文件；若使用 `--env-file` 且值为相对路径，则相对该 `.env` 文件目录解析；安装后的独立 CLI 在未显式配置时会默认指向 `~/.agents/skills/stock-cache/.runtime/last-write-status.txt` |
| `INDEX_LIST_PATH` | 否 | `.runtime/default-indexes.csv` | 默认指数清单 CSV 路径；若使用 `--env-file` 且值为相对路径，则相对该 `.env` 文件目录解析；安装后的独立 CLI 在未显式配置时会默认指向 `~/.agents/skills/stock-cache/.runtime/default-indexes.csv`，仓库内开发场景则回退到 `runtime/default-indexes.csv` |
| `ALLOW_INDICATOR_BACKFILL_ON_READ` | 否 | `true` | 读取时是否允许指标回填 |
| `ENABLE_TUSHARE_INDICATORS` | 否 | `true` | provider 指标开关 |
| `ENABLE_LOCAL_INDICATOR_FALLBACK` | 否 | `true` | 是否允许本地指标兜底逻辑 |
| `WRITE_BATCH_SIZE` | 否 | `500` | repository 单次 upsert 的最大批量行数 |
| `LOG_LEVEL` | 否 | `INFO` | 日志级别 |

示例文件见 [`.env.example`](.env.example)。

配置优先级如下：

1. shell `export`，例如 `export TUSHARE_TOKEN=...`
2. `--env-file /path/to/.env`
3. 当前工作目录下默认的 `.env`
4. [src/config.py](src/config.py) 中定义的字段默认值

`uv run stock-cache config show` 会在解析并应用优先级规则后，以 `ENV_NAME=value` 的形式输出每个配置变量的最终运行时值。

## 数据库初始化

执行：

```bash
uv run stock-cache init-db
uv run stock-cache --env-file /path/to/.env init-db
```

该命令会加载 [src/db/schema.sql](src/db/schema.sql)，确保核心数据表存在，并输出如下 JSON：

```json
{
  "status": "ok",
  "created_tables": ["daily_cyq_chips", "daily_cyq_perf", "daily_index", "daily_indicators", "daily_market", "instruments", "job_runs"],
  "already_present": [],
  "missing": []
}
```

核心表包括：

- `instruments`
- `daily_index`
- `daily_market`
- `daily_indicators`
- `daily_cyq_chips`
- `daily_cyq_perf`
- `job_runs`

## 写入流程

执行同步任务：

```bash
uv run stock-cache write --mode full
uv run stock-cache --env-file /path/to/.env write --mode full
```

只同步默认指数清单中的指数：

```bash
uv run stock-cache write --mode indexes \
  --start-date 2025-01-01 \
  --end-date 2026-05-04
uv run stock-cache --env-file /path/to/.env write \
  --mode indexes \
  --start-date 2025-01-01 \
  --end-date 2026-05-04
```

按 `ts_code` 同步单只股票：

```bash
uv run stock-cache write --mode single --ts-code 000001.SZ
uv run stock-cache --env-file /path/to/.env write --mode single --ts-code 000001.SZ
```

或者通过名称从已缓存的 `instruments` 表中解析单只股票：

```bash
uv run stock-cache write --mode single --name 平安银行
uv run stock-cache --env-file /path/to/.env write --mode single --name 平安银行
```

通过 CLI 覆盖默认的近期交易日窗口：

```bash
uv run stock-cache write --mode full --lookback-trading-days 30
uv run stock-cache --env-file /path/to/.env write --mode full --lookback-trading-days 30
```

或者同步指定的绝对交易日范围：

```bash
uv run stock-cache write --mode full \
  --start-date 2026-01-01 \
  --end-date 2026-03-31
uv run stock-cache --env-file /path/to/.env write \
  --mode full \
  --start-date 2026-01-01 \
  --end-date 2026-03-31
```

CLI 支持三种 `--mode` 值：

- `full`：同步所选窗口内的全部活跃股票，并在股票阶段完成后同步默认指数清单
- `single`：通过 `--ts-code` 或 `--name` 精确同步一只股票
- `indexes`：只同步默认指数清单中的指数，不拉取股票数据

`write --mode full` 还会读取 `INDEX_LIST_PATH` 指向的 CSV，并同步其中 `enabled=true` 的指数日线。若通过 `--env-file` 提供相对路径，CLI 会按该 `.env` 文件所在目录解析；未显式配置时，安装后的独立运行时默认清单是 `~/.agents/skills/stock-cache/.runtime/default-indexes.csv`，仓库内开发场景则回退到 [runtime/default-indexes.csv](runtime/default-indexes.csv)。

如果只想运行指数阶段，可以直接使用 `write --mode indexes`。它复用相同的日期窗口规则，但不会拉取股票数据。

写入执行期间，CLI 会向 `stderr` 输出进度信息，便于查看当前阶段；最终供机器读取的任务摘要仍会输出到 `stdout`。成功执行时示例如下：

```json
{
  "job_id": "20260331T120000Z",
  "status": "success",
  "started_at": "2026-03-31T12:00:00+00:00",
  "finished_at": "2026-03-31T12:00:01+00:00",
  "total_symbols": 1,
  "success_count": 1,
  "failed_count": 0
}
```

每次写入执行还会覆盖 `STATUS_FILE_PATH` 指向的状态文件。该文件包含便于人工阅读的摘要信息，包括计数以及成功、失败的股票列表。

在 `write --mode full` 模式下，CLI 会按交易日逐个拉取数据，立即对当前交易日做规范化处理，并按 `WRITE_BATCH_SIZE` 控制的分块方式持久化行数据。这样写入时的内存占用只与当前交易日载荷和当前 repository 批次有关，而不会增长到覆盖整个写入窗口。

指数同步会按 `INDEX_LIST_PATH` 中配置的 `ts_code` 与 `group_name` 逐个拉取：

- `major` 组默认走 `index_daily`
- `sw_secondary` 和 `theme` 组默认走 `sw_daily`

CSV 最低字段要求如下：

```csv
ts_code,name,group_name,enabled
000300.SH,沪深300,major,true
801012.SI,农产品加工(申万),sw_secondary,true
801250.SI,申万制造,theme,true
```

写入窗口规则如下：

- `uv run stock-cache write --mode full` 使用 `DEFAULT_LOOKBACK_TRADING_DAYS`
- `uv run stock-cache write --mode single --ts-code 000001.SZ` 使用相同的写入窗口规则，但仅作用于该股票
- `--ts-code` 和 `--name` 只能与 `--mode single` 一起使用
- `--mode single` 必须且只能提供 `--ts-code` 或 `--name` 其中一个
- `--lookback-trading-days` 只会覆盖当前命令的 `DEFAULT_LOOKBACK_TRADING_DAYS`
- `--start-date` 和 `--end-date` 必须同时提供
- `--lookback-trading-days` 不能与 `--start-date` / `--end-date` 组合使用

## 缓存统计

使用 `stats date-range` 查看各表中可查询的缓存交易日区间：

```bash
uv run stock-cache stats date-range
```

响应结构：

```json
{
  "data": {
    "daily_market": {
      "min_trade_date": "2026-01-02",
      "max_trade_date": "2026-03-31",
      "continuous_ranges": [
        ["2026-01-02", "2026-01-05", "2026-01-06"],
        ["2026-03-31"]
      ]
    },
    "daily_indicators": {
      "min_trade_date": "2026-01-02",
      "max_trade_date": "2026-03-31",
      "continuous_ranges": [
        ["2026-01-02", "2026-01-05", "2026-01-06"],
        ["2026-03-31"]
      ]
    },
    "daily_index": {
      "min_trade_date": "2026-01-02",
      "max_trade_date": "2026-03-31",
      "continuous_ranges": [
        ["2026-01-02", "2026-01-05", "2026-01-06"],
        ["2026-03-31"]
      ]
    },
    "daily_cyq_chips": {
      "min_trade_date": "2026-01-02",
      "max_trade_date": "2026-03-31",
      "continuous_ranges": [
        ["2026-01-02", "2026-01-05", "2026-01-06"],
        ["2026-03-31"]
      ]
    },
    "daily_cyq_perf": {
      "min_trade_date": "2026-01-02",
      "max_trade_date": "2026-03-31",
      "continuous_ranges": [
        ["2026-01-02", "2026-01-05", "2026-01-06"],
        ["2026-03-31"]
      ]
    }
  }
}
```

`continuous_ranges` 是一个二维数组，表示按连续交易日区段分组后的实际缓存交易日。它不会假设全局最小日期和最大日期之间的数据一定完整。

## 删除缓存数据

删除单个缓存交易日：

```bash
uv run stock-cache delete by-date --trade-date 2026-03-31
```

删除一个缓存日期范围：

```bash
uv run stock-cache delete by-date \
  --start-date 2026-01-01 \
  --end-date 2026-01-31
```

删除响应结构：

```json
{
  "query": {
    "start_date": "2026-01-01",
    "end_date": "2026-01-31"
  },
  "data": {
    "daily_market_deleted": 12,
    "daily_indicators_deleted": 9,
    "daily_index_deleted": 3,
    "daily_cyq_chips_deleted": 30,
    "daily_cyq_perf_deleted": 6
  },
  "meta": {
    "total_deleted_rows": 60
  }
}
```

## 读取原始数据

使用 `read raw` 获取单只股票在指定日期范围内的缓存历史数据：

```bash
uv run stock-cache read raw \
  --ts-code 000001.SZ \
  --start-date 2026-01-01 \
  --end-date 2026-03-30
```

也可以通过缓存 `instruments` 表中的精确股票名称进行解析：

```bash
uv run stock-cache read raw \
  --name "Ping An Bank" \
  --start-date 2026-01-01 \
  --end-date 2026-03-30
```

`--ts-code` 和 `--name` 必须且只能提供一个。

响应结构：

```json
{
  "query": {
    "ts_code": "000001.SZ",
    "start_date": "2026-01-01",
    "end_date": "2026-03-30"
  },
  "data": {
    "market": [],
    "indicators": [],
    "indexes": [],
    "cyq_chips": [],
    "cyq_perf": []
  },
  "meta": {
    "row_count_market": 0,
    "row_count_indicators": 0,
    "row_count_indexes": 0,
    "row_count_cyq_chips": 0,
    "row_count_cyq_perf": 0
  }
}
```

`market`、`indicators`、`indexes`、`cyq_chips` 和 `cyq_perf` 都是从 PostgreSQL 缓存中序列化得到，日期值会以 ISO 字符串形式输出。

`init-db`、`write`、`read raw` 和 `read screen` 在继续执行前都会先检查 PostgreSQL 是否可达。如果配置的 `POSTGRES_DSN` 无法连接，CLI 会以类似如下的 JSON 退出：

```json
{
  "status": "error",
  "error": "postgres_unreachable",
  "message": "PostgreSQL is not reachable at configured POSTGRES_DSN."
}
```

## 读取筛选数据

使用 `read screen` 按交易日和筛选阈值查询缓存数据集：

```bash
uv run stock-cache read screen \
  --trade-date 2026-03-30 \
  --pct-chg-gte 5 \
  --turnover-rate-gte 3 \
  --macd-gte 0
```

可用筛选项：

- `--pct-chg-gte`
- `--turnover-rate-gte`
- `--total-mv-gte`
- `--total-mv-lte`
- `--macd-gte`
- `--kdj-j-gte`

响应结构：

```json
{
  "query": {
    "trade_date": "2026-03-30",
    "filters": {
      "pct_chg_gte": 5.0,
      "turnover_rate_gte": 3.0,
      "macd_gte": 0.0
    }
  },
  "data": [
    {
      "ts_code": "300001.SZ",
      "trade_date": "2026-03-30",
      "pct_chg": 5.0,
      "turnover_rate": 3.0,
      "macd": 0.0
    }
  ],
  "meta": {
    "matched": 1
  }
}
```

## 开发工作流

常用命令：

```bash
uv run stock-cache --help
uv run pytest
uv run pytest tests/test_cli.py tests/test_config.py
```

本地开发时，建议优先遵循以下顺序：

1. 先为变更行为补充或更新聚焦测试
2. 先运行有针对性的 pytest 命令
3. 变更区域稳定后，再进行更大范围的验证

## 仓库结构

- [src/cli.py](src/cli.py): CLI 入口
- [src/config.py](src/config.py): 配置项
- [src/db/](src/db): schema 与 PostgreSQL 辅助代码
- [src/providers/](src/providers): 上游 provider 集成
- [src/repositories/](src/repositories): 持久化层
- [src/services/](src/services): 规范化、重试、状态文件支持
- [src/use_cases/](src/use_cases): 写入和读取命令的应用流程
- [tests/](tests): 单元测试与集成测试

面向代码代理的仓库级说明见 [AGENTS.md](AGENTS.md)。
