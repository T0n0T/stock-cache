# v3 程序设计

v3 是独立的 Rust + DuckDB 路径。Tushare HTTP 拉取、JSON 解码、批量写入、MACD/KDJ/rolling/left-peak/CZSC 计算和结果落库均由 Rust 完成；仓库不再维护旧数据库后端或兼容写入路径。

## 程序边界

```text
Rust reqwest Tushare transport
        |
        v
stock-cache (Rust) ---- DuckDB file
        |
        +-- source tables
        +-- calc tables and checkpoints
```

Rust CLI 是运行和消费 v3 的唯一命令入口；`tools/*.py` 只承担字段发现、metadata/文档生成和基准辅助，不进入生产数据路径。`duckdb-neo` 提供 DuckDB v2 C API binding。

## Rust writer

`src/v3/duckdb.rs` 对每个批次使用一条连接和一个事务：

1. 用 `read_json_auto(..., format='newline_delimited')` 将 JSONL 载入临时 staging 表；
2. 按目标表逻辑键保留批次最后一行；
3. `append` 检查目标表中不存在重复逻辑键；
4. `replace-dates` 删除显式日期切片后整体插入，空批次可以表达确认空日；
5. `replace-keys` 只处理调用方给出的完整键；
6. `replace-batch-keys` 按 staging 中出现的逻辑键批量删除并插入，用于全量计算聚合写入；
7. 成功提交后执行 `CHECKPOINT`，异常回滚。

对应策略为：日常新增使用 `append`，历史补全使用按日期分块的 `replace-dates`，单日修订也使用单日期 `replace-dates`；需要精确删改时使用 `replace-keys`。无 `--changed-from` 的全量计算按计算表聚合，再用 `replace-batch-keys` 精确替换输入批次涉及的键，避免逐标的打开 writer，也不误删同日其他标的。生产路径不逐行 upsert。

## CLI

```bash
cargo run --bin stock-cache -- init --database ./runtime/stock-cache.duckdb
cargo run --bin stock-cache -- status --database ./runtime/stock-cache.duckdb
cargo run --bin stock-cache -- fetch --database ./runtime/stock-cache.duckdb \
  --dataset stock_stk_factor_pro --params '{"trade_date":"20261113"}'
cargo run --bin stock-cache -- import --database ./runtime/stock-cache.duckdb \
  --table source_daily_bars --input bars.jsonl --strategy append
cargo run --bin stock-cache -- calc --input bars-with-warmup.jsonl --calendar trade_cal.txt \
  --database ./runtime/stock-cache.duckdb --write
```

源数据修订时，以 `--changed-from` 指定起点并提供足够 warmup；计算器根据 checkpoint 规划受影响日期，再由 Rust writer 整体替换结果切片。

## 计算正确性

MACD 使用与 `pandas.Series.ewm(adjust=False)` 相同的递推：12/26 EMA、9 DEA，首个有效收盘作种子。周/月周期按交易日历闭合；未完成周期只产生 as-of 预览，不推进完成状态。KDJ 使用 N=9 和 1/3 平滑；rolling、left-peak、CZSC 均按 point-in-time 输入计算。

测试覆盖 EMA 黄金值、跨年周/月、节假日、停牌和空值、源数据修订后的 checkpoint 重算、未来数据不改变既有 as-of，以及周/月预览不重复累计。

## 最近一周真实拉取指标

数据集为 `stock_stk_factor_pro`，日期为 2026-11-09 至 2026-11-13，5/5 请求成功，拉取 27,864 行，按逻辑键去重后写入 27,452 行，JSONL 输入 140.59 MiB。

| 阶段 | 中位数 | P95 |
| --- | ---: | ---: |
| HTTP 请求与 JSON 解码 | 4,521.7 ms | 4,680.4 ms |
| JSONL 序列化 | 5,696.2 ms | 5,778.7 ms |
| DuckDB 写入与 checkpoint | 1,059.4 ms | 1,194.3 ms |
| `fetch` 命令内部总耗时 | 11,378.3 ms | 11,556.5 ms |
| 含进程启动 wall time | 11,592.6 ms | 11,780.5 ms |

因此此前所说的约 11 秒包含抓取、解码、序列化、DuckDB 写入和 checkpoint，**不包含 MACD/KDJ/CZSC 计算**。端到端中位吞吐约 489.9 fetched rows/s，DuckDB 写入约 5,260.4 written rows/s；来源批次中的重复逻辑键解释了两者的行数差异。

## 写入性能测量

基准工具 `tools/benchmark_rust_writer.py` 默认把 JSON 写到 `target/` 下的临时目录，不把生成报告放入仓库。报告至少区分：

- writer-only wall time、逻辑行数和 rows/s；
- fetch 端到端时间，拆分 HTTP/解码、序列化、写入/checkpoint；
- 每日追加、历史日期替换、单日替换和空库初建；
- 冷启动与同进程重复运行，分别给中位数和 P95。

真实周拉取的汇总指标保留在本文，原始 JSON/Markdown 报告属于本地生成物，不纳入提交。

## 五日计算与计算表写入

使用真实 `stock_stk_factor_pro` 数据，日期为 2026-11-09 至 2026-11-13，共 27,452 行、5,575 个标的。输入只有这五个交易日，没有额外 warmup 行；单独计算命令耗时 1.63 秒。`stock-cache calc --write` 同时计算并写入六张计算表，输出 159,544 行、约 136.7 MiB；干净计算表首次填入耗时 19.73 秒，两次同批次替换分别耗时 20.06 和 20.29 秒，三轮中位数 20.06 秒，峰值 RSS 约 1.39 GiB。计算表最终行数为：`calc_period_bars` 11,142、`calc_indicator_state` 38,594，其余四张日表各 27,452。

单独计算与计算加写入的差值约 18–20 秒，但包含结果解析、按表暂存、DuckDB 替换/checkpoint 和最终输出，不能视作纯数据库写耗时。该路径当前仍会在内存中保留全量渲染结果；本轮峰值内存显示后续应优先把计算结果流式写入按表暂存文件，降低批量回补的内存占用。此零 warmup 运行只用于性能测量，MACD 等滚动指标从给定五日样本起算，不代表长历史基准值。

## 兼容的市场缓存主入口

`stock-cache write --mode full --lookback-trading-days 1` 恢复 v2 的主流程语义：
用交易日历选择截至上海当前日期的最近一个交易日，更新该日配置中的来源表；
为股票、配置指数、同花顺指数三类计算来源补齐此前 36 个月缺失历史，计算并
写入目标日期六类结果。非交易日选择最近交易日；当日数据尚未发布时明确失败。
交易日历请求延伸到目标日期之后 40 个自然日，禁止把输入截断日当成周/月收盘日。

`--start-date/--end-date` 与 `--lookback-trading-days` 互斥。`--mode cache` 仅缓存
目标日期，`--mode calc` 仅使用已有来源和已缓存日历计算。默认数据库为
`runtime/stock-cache.duckdb`，支持 `DUCKDB_PATH`、`--database`；加载 `.env` 或
`--env-file`，环境变量优先。`INDEX_LIST_PATH` 和 `TARGET_CACHE_TABLES_PATH`
可覆盖内置配置。JSONL 导入改用 `import` 子命令。

来源表按请求日期及指数代码替换；历史覆盖由 `cache_fetch_coverage` 记录，
包括已成功获取的空响应，避免每天重复请求。目标日期始终刷新。计算按标的批次
从 DuckDB 读取全部已缓存历史；股票使用前复权 OHLC，指数使用原始 OHLC。
预热仅参与计算，输出在写入前按目标日期过滤，避免把全量历史结果留在内存。
`--chunk-size` 控制标的批次，`--max-concurrency` 控制 HTTP 并发。
`--force-recalc` 在 full 模式额外刷新历史；计算本身每次都会重算目标切片。
首轮 MACD 沿用 v2 的 36 个月有限预热口径，不能视为上市以来的精确基线。
每批来源与计算结果使用 writer 事务；跨 HTTP 和各计算批次不构成单一事务，失败
不会报告成功，已完成的来源缓存可供重试复用。
