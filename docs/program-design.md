# 程序设计

v3 是独立的 Rust + DuckDB 路径。Tushare HTTP 拉取、JSON 解码、批量写入、MACD/KDJ/rolling/left-peak/CZSC 计算和结果落库均由 Rust 完成；仓库不再维护旧数据库后端或兼容写入路径。

## 程序边界与目录分层

```text
Rust reqwest Tushare transport
        |
        v
stock-cache (Rust) ---- DuckDB file
        |
        +-- source tables
        +-- calc tables and checkpoints
```

Rust CLI 是唯一命令入口；`tools/*.py` 只承担字段发现、metadata/文档生成和基准辅助，不进入生产数据路径。`duckdb-neo` 提供 DuckDB v2 C API binding。

代码按职责分层：`src/cli` 定义命令参数，`src/app` 编排写入、擦除、计算和网络服务流程，`src/calc` 实现可复用的计算算法，`src/source` 负责 Tushare HTTP，`src/storage/duckdb` 管理表结构、批量写入、状态读取和 Quack 服务，`src/metadata` 提供数据集元信息。计算算法不再放在单独的版本目录中。

## Rust writer

`src/storage/duckdb/writer.rs` 对每个批次使用一条连接和一个事务：

1. 用 `read_json_auto(..., format='newline_delimited')` 将 JSONL 载入临时 staging 表；
2. 按目标表逻辑键保留批次最后一行；
3. `append` 检查目标表中不存在重复逻辑键；
4. `replace-dates` 删除显式日期切片后整体插入，空批次可以表达确认空日；
5. `replace-keys` 只处理调用方给出的完整键；
6. `replace-batch-keys` 按 staging 中出现的逻辑键批量删除并插入，用于全量计算聚合写入；
7. 成功提交后执行 `CHECKPOINT`，异常回滚。

对应策略为：日常新增使用 `append`，历史补全使用按日期分块的 `replace-dates`，单日修订也使用单日期 `replace-dates`；需要精确删改时使用 `replace-keys`。主流程的计算按计算表聚合，再用 `replace-batch-keys` 精确替换输入批次涉及的键，避免逐标的打开 writer，也不误删同日其他标的。生产路径不逐行 upsert。

## CLI

```bash
cargo run --bin stock-cache -- init --database ./runtime/stock-cache.duckdb
cargo run --bin stock-cache -- status --database ./runtime/stock-cache.duckdb
cargo run --bin stock-cache -- write --mode full --lookback-trading-days 1
cargo run --bin stock-cache -- erase --start-date 2026-11-09 --end-date 2026-11-13
cargo run --bin stock-cache -- serve --database ./runtime/stock-cache.duckdb
```

历史修订通过 `write --mode full --start-date ... --end-date ...` 重拉并计算；结果依赖后续日期时，应将目标范围延伸至最新缓存日期。

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

基准工具 `tools/benchmark_rust_writer.py`（先执行 `cargo build --release --example writer-benchmark`）使用 `calc_daily_indicators` 测量批量 writer，默认把 JSON 写到 `target/` 下的临时目录，不把生成报告放入仓库。报告至少区分：

- writer-only wall time、逻辑行数和 rows/s；
- fetch 端到端时间，拆分 HTTP/解码、序列化、写入/checkpoint；
- 每日追加、历史日期替换、单日替换和空库初建；
- 冷启动与同进程重复运行，分别给中位数和 P95。

真实周拉取的汇总指标保留在本文，原始 JSON/Markdown 报告属于本地生成物，不纳入提交。

## 五日计算与计算表写入

使用真实 `stock_stk_factor_pro` 数据，日期为 2026-11-09 至 2026-11-13，共 27,452 行、5,575 个标的。输入只有这五个交易日，没有额外 warmup 行；单独计算命令耗时 1.63 秒。当时的独立计算入口（现已收回 write 内部） 同时计算并写入六张计算表，输出 159,544 行、约 136.7 MiB；干净计算表首次填入耗时 19.73 秒，两次同批次替换分别耗时 20.06 和 20.29 秒，三轮中位数 20.06 秒，峰值 RSS 约 1.39 GiB。计算表最终行数为：`calc_period_bars` 11,142、`calc_indicator_state` 38,594，其余四张日表各 27,452。

单独计算与计算加写入的差值约 18–20 秒，但包含结果解析、按表暂存、DuckDB 替换/checkpoint 和最终输出，不能视作纯数据库写耗时。该路径当前仍会在内存中保留全量渲染结果；本轮峰值内存显示后续应优先把计算结果流式写入按表暂存文件，降低批量回补的内存占用。此零 warmup 运行只用于性能测量，MACD 等滚动指标从给定五日样本起算，不代表长历史基准值。

## 兼容的市场缓存主入口

`stock-cache write --mode full --lookback-trading-days 1` 恢复 v2 的主流程语义：
用交易日历选择截至上海当前日期的最近一个交易日，更新该日配置中的来源表；
为股票和配置指数两类计算来源补齐此前 36 个月缺失历史，计算并写入目标日期
六类结果；同花顺板块日线仍作为来源表缓存，但不再参与指标计算。非交易日选择最近交易日；当日数据尚未发布时明确失败。
交易日历请求延伸到目标日期之后 40 个自然日，禁止把输入截断日当成周/月收盘日。

`--start-date/--end-date` 与 `--lookback-trading-days` 互斥。`--mode cache` 仅缓存
目标日期，`--mode calc` 仅使用已有来源和已缓存日历计算。默认数据库为
`runtime/stock-cache.duckdb`，支持 `DUCKDB_PATH`、`--database`；加载 `.env` 或
`--env-file`，环境变量优先。`INDEX_LIST_PATH` 和 `TARGET_CACHE_TABLES_PATH`
可覆盖内置配置。公开子命令为 init/init-db、status、write、erase、serve。

来源表按请求日期及指数代码替换；历史覆盖由 `cache_fetch_coverage` 记录，
包括已成功获取的空响应，避免每天重复请求。目标日期默认始终刷新。计算按标的批次
从 DuckDB 读取全部已缓存历史；股票使用前复权 OHLC，指数使用原始 OHLC。
预热仅参与计算，输出在写入前按目标日期过滤，避免把全量历史结果留在内存。
`--chunk-size` 控制标的批次，`--max-concurrency` 控制 HTTP 并发。
Tushare HTTP 请求默认按每分钟 450 次限流，分页与重试共享同一预算；`write` 还按该预算切分抓取批次，并在批次之间等待完整窗口，避免补算任务一次性超过接口额度。可用 `TUSHARE_HTTP_REQUESTS_PER_MINUTE` 调低或调整预算。
`--force-recalc` 在 full 模式额外刷新历史；计算本身每次都会重算目标切片。
`--skip-covered`（别名 `--resume`）跳过 `cache_fetch_coverage` 已覆盖的请求，
目标日期也包含在内，用于失败后断点续跑；它与 `--force-recalc` 互斥。
首轮 MACD 沿用 v2 的 36 个月有限预热口径，不能视为上市以来的精确基线。
每批来源与计算结果使用 writer 事务；跨 HTTP 和各计算批次不构成单一事务，失败
不会报告成功，已完成的来源缓存可供重试复用。

## 申万 2021 二级行业资金流

`stock_moneyflow` 保存 Tushare `moneyflow` 的按单笔金额分档数据（小/中/大/特大单的买卖量与金额）。`index_sw_classify` 和 `index_sw_member` 由 `write --mode cache --refresh` 维护；后者合并 `is_new=Y` 当前成分与 `is_new=N` 历史成分，保留 `in_date`/`out_date`，因此可以按时点归属，而不是只用当前快照。

`config/default-indexes.csv` 默认包含 124 个已发布指数的申万 2021 二级行业代码（`801xxx.SI`）。`index_idx_factor_pro` 对每个指数按连续交易日区间抓取：一个指数的一段目标窗口只发一次请求，响应按 30 个交易日分块写入并逐日记录 `cache_fetch_coverage`。因此 124 个行业指数补齐 2020 年以来的历史只需百余次请求，而不是 124 × 交易日数次。

`calc_daily_industry_moneyflow` 是本地派生表：用 `stock_moneyflow` 与 `index_sw_member` 的时点有效成分聚合，版本 `sw2021-l2-v1`。它在 cache/full/calc 三种模式的目标窗口结束后幂等重建；窗口内有 `stock_moneyflow` 但 `index_sw_member` 为空时直接报错，提示先运行 `--refresh`。行业净额之和等于当天所有被映射股票的净额之和；未被申万成分覆盖的股票（新股、北交所、退市）不进入合计。SW2021 二级分类共 134 个代码，其中 124 个 `is_pub=1` 且有指数因子数据；其余 10 个不发布指数，不加入默认指数列表。

## 市场指数权重

`index_weight` 按月保存配置中 `category=market` 的 9 个市场指数成分权重。`--refresh` 时按指数、按年分块抓取 2020 年以来的月末快照，使用 `replace-batch-keys` 只替换本批出现的键，单指数单年数据量控制在 Tushare 的 10 万行 offset 上限以内。权重表只用于成分归因和回测，不进入 MACD/KDJ/CZSC 计算；申万成分仍由 `index_sw_member` 独立维护。

## 日期擦除

`stock-cache erase --start-date YYYY-MM-DD --end-date YYYY-MM-DD` 恢复 v2 的明确
日期区间擦除语义。一个事务删除所有日期来源表、六张计算表和
cache_fetch_coverage 的区间记录；周/月表按 period_end_date 匹配。保留 stock_basic、
index_ths、index_ths_member 和 cache_trade_calendar。日期必须成对提供且起日不晚于
止日，缺失数据库报错。另可用 `--lookback-trading-days N` 指定相对窗口：从
cache_trade_calendar 中取不晚于今天（东八区）的最近 N 个交易日，与显式日期参数
互斥，历史交易日不足时报错并提示先运行 write。区间外结果不删除；历史改动后需
重算依赖它的后续日期。计算和 HTTP 拉取均作为 write 内部能力，不提供
import/fetch/calc 子命令。

## Quack 网络服务

`stock-cache serve` 在同一个 Rust 可执行文件中封装 DuckDB 原生 Quack 服务；不再增加启动脚本或单独的 DuckDB CLI 依赖。它通过协议传输 SQL 查询结果，不负责复制数据库文件。

职责按现有目录分层：`src/cli/serve.rs` 定义参数并解析环境变量，`src/app/serve.rs` 检查文件、监听地址和认证要求，管理前台进程与退出信号；`src/storage/duckdb/quack.rs` 通过 `duckdb-neo` 管理 Environment、Instance、只读数据库 attachment 和服务生命周期。

数据库默认 `runtime/stock-cache.duckdb`，优先级为 `--database`、`DUCKDB_PATH`、默认值。文件必须已存在，失败时不创建目录或空数据库。默认绑定 `127.0.0.1:9494`；`--bind` 覆盖 `QUACK_BIND`。`--auth-token` 覆盖 `QUACK_AUTH_TOKEN`，所有监听地址都要求非空 token，避免原生服务自动生成凭证却无法交付给调用方。统一支持 `.env` 和全局 `--env-file`，进程环境优先于文件。

服务在独立 DuckDB Instance 中以 `read_only=true` 挂载目标文件为默认数据库，再执行 `CALL quack_serve('quack:host:port', token = '...', allow_other_hostname = true)`；允许用户显式选择的网络监听地址。传输遵循 DuckDB 原生默认值：localhost 使用 HTTP，网络监听使用 HTTPS 和自动生成的证书。读取 `quack_serve` 返回的 URL，以及 `quack_server_list().info` 中的 `ssl_fingerprint`，供客户端固定证书。服务启动成功且退出信号处理器已就绪后，输出包含地址、URL、数据库、只读标志、认证状态、TLS 标志和证书指纹的 JSON，启动 JSON 不输出 token。

Ctrl-C/SIGINT 或 SIGTERM 触发 `CALL quack_stop(...)`，成功后进程正常退出；服务对象销毁时也会尝试停止监听。服务持有数据库读锁，独立进程的本地写入会失败。更新顺序为停止服务、执行 `write` 或 `erase`、再启动服务；需要持续在线且同时更新时，应另行设计同进程写入或快照发布机制。

客户端使用支持 Quack 的 DuckDB 执行 `ATTACH 'quack:server-host:9494' AS market (TOKEN '...', SSL_FINGERPRINT '...')`，通过 `market.main.<table>` 查询。证书指纹以本次启动输出为准；连接默认本机 HTTP 入口时省略 `SSL_FINGERPRINT`，保留 token。集成测试复用 `duckdb-neo` 的原生客户端，覆盖真实网络查询、网络监听的 TLS 与证书固定、缺失/错误 token、包含单引号的 token、远程写入拒绝、配置文件、SIGINT/SIGTERM 退出和退出后恢复本地写入，无需增加客户端 crate。systemd/Docker 等只负责守护该前台命令。
