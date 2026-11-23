# stock-cache

`stock-cache` 是一个由 Rust 实现的 Tushare 数据采集、DuckDB 存储与本地因子计算工具。当前开发目标是 v3，使用本地 DuckDB 文件。

Rust 可执行程序负责 HTTP 请求、JSON 解码、批量写入、MACD/KDJ/rolling/left-peak/CZSC 计算以及 DuckDB 事务。

## 目录结构

```text
src/main.rs                  CLI 入口与命令分发
src/cli/                     命令行参数定义
src/app/                     write、erase、serve、status 与计算工作流
src/calc/                    MACD、KDJ、rolling、left-peak 与 CZSC 算法
src/source/                  Rust Tushare HTTP 客户端
src/storage/duckdb/          DuckDB schema、写入器、缓存、检查点与 Quack 服务端
src/metadata/                数据集与字段元数据
schemas/                     数据源元数据定义
config/                      默认缓存表与索引列表
docs/program-design.md       运行时与写入策略
docs/schema-design.md        表结构与性能设计
docs/data-usage-guide.md     DuckDB 消费端查询
docs/postgres-migration.md   旧 PostgreSQL 一次性迁移
```

`app` 负责编排工作流，`calc` 包含可复用的计算逻辑，`storage` 负责数据库操作。目录名称描述了这些边界；计算实现不绑定于某个 schema 或发布版本。

## 每日行情缓存工作流

```bash
target/release/stock-cache write --mode full --lookback-trading-days 1
# 指定历史窗口：
target/release/stock-cache write --mode full --start-date 2026-11-09 --end-date 2026-11-13
```

`write` 是常规的行情缓存入口。`full` 会抓取目标日的行情数据源，补齐前 36 个月缺失的计算数据源历史，然后计算并写入目标结果。交易日期取自 Tushare 的上交所日历；遇到周末时选择最近的一个交易日。如果目标日的股票数据尚未发布，命令会直接失败，而不会静默使用前一日的数据。

默认数据库为 `runtime/stock-cache.duckdb`（可用 `DUCKDB_PATH` 或 `--database` 覆盖）。Shell 变量优先于 `.env`；`--env-file` 用于指定特定文件。`cache` 只抓取目标数据源；`calc` 基于已缓存的数据源和日历进行计算，不访问 HTTP。`--refresh` 会更新参考数据快照。`--max-concurrency` 限制 HTTP 请求并发数；`--chunk-size` 限制每批计算的标的数量。Tushare 请求默认限制为每分钟 450 次（可用 `TUSHARE_HTTP_REQUESTS_PER_MINUTE` 调整），分页和重试也计入限制；写入批次达到预算后会自动等待。`--force-recalc` 还会在 `full` 模式下重新抓取历史数据。默认情况下每次运行都会刷新目标数据；成功的预热请求会被复用，包括空响应。新增的指数会获得各自的历史回填。`--skip-covered`（别名 `--resume`）还会复用已覆盖的目标请求，因此被中断的多日回填可以继续执行，无需重新抓取已完成的日期；它与 `--force-recalc` 冲突。

股票使用 `*_qfq` 复权 OHLC；指数使用原始 OHLC。预热行保留在数据源表中；只有目标日的计算行以及以目标窗口结束的完整周期行会被持久化。已有的更早缓存历史仍会作为计算输入，以免 EMA 种子每天向前漂移。初始的 36 个月预热遵循 v2 的有限历史约定，而不是从上市首日开始的历史基线。写入采用 Rust 批量暂存和事务；失败时已完成的源数据批次仍可复用于重试。一个工作流并不是跨所有 HTTP 请求和计算批次的单一事务。

### 申万二级行业资金流

`stock_moneyflow` 缓存 Tushare `moneyflow` 的小/中/大/特大单买卖数据。`config/default-indexes.csv` 默认加入 124 个申万 2021 二级行业指数（`801xxx.SI`），其行情和因子写入现有 `index_idx_factor_pro`。`index_sw_classify` / `index_sw_member` 由 `--refresh` 维护，后者同时保存当前成分和历史成分的 `in_date` / `out_date`。`--refresh` 还会按月抓取 9 个市场指数的 `index_weight` 成分权重（2020 年起），供归因和回测查询，不参与指标计算。

`write` 在 cache/full/calc 三种模式结束目标窗口后，会幂等重建 `calc_daily_industry_moneyflow`：按申万二级行业聚合个股资金流，版本 `sw2021-l2-v1`。该表每天约 131 个行业（包含 7 个有成分但不发布指数的二级行业），行业净额之和等于当天被映射股票的净额之和。首次使用先执行 `write --mode cache --refresh` 获取申万分类和成分，再做历史回填。

公开命令为 `init`（别名 `init-db`）、`status`、`write`、`erase` 和 `serve`。计算与 HTTP 抓取是 `write` 的内部阶段。

## 快速开始

```bash
cargo build --bins
cargo run --bin stock-cache -- init --database ./runtime/stock-cache.duckdb
cargo run --bin stock-cache -- status --database ./runtime/stock-cache.duckdb
```

删除一个闭区间日期窗口：

```bash
stock-cache erase --start-date 2026-11-09 --end-date 2026-11-13
# 或删除最近 N 个缓存交易日（截止今天，基于 cache_trade_calendar）：
stock-cache erase --lookback-trading-days 5
```

删除操作会在一个事务中移除带日期的数据源行、计算行（周期表按周期结束日）以及请求覆盖记录。它会保留参考数据快照和交易日历。显式窗口的两个日期必须成对提供；`--lookback-trading-days` 与显式日期互斥，并且要求数据库里已有足够的交易日历（先运行过 `write`）。不存在隐式的全库删除。更晚的计算日期仍会保留（与 v2 一致）；在修正历史数据后，请重算至最近一个缓存日期，因为滚动结果可能依赖该历史。

## 从旧 PostgreSQL 迁移

如需一次性迁移旧库中的 Tushare 源表和参考表，使用开发辅助示例：

```bash
cargo run --release --example import-postgres -- \
  --dsn 'postgresql://user:password@host:5432/db' \
  --database runtime/stock-cache.duckdb
```

按区间迁移时加 `--since`，`--until` 缺省取目标 DuckDB 的最新日期：

```bash
cargo run --release --example import-postgres -- \
  --dsn 'postgresql://user:password@host:5432/db' \
  --since 2024-01-01 \
  --database runtime/stock-cache.duckdb
```

不希望覆盖已有行时加 `--insert-only`，它只补齐缺失的键。它只导入源数据并重建覆盖记录，不迁移 `calc_*` 计算结果；计算由 Rust 实现重算。详见 `docs/postgres-migration.md`。

## 通过 Quack 进行网络访问

在环境变量或 `.env` 中设置非空的 `QUACK_AUTH_TOKEN`，然后在写完缓存后在前台启动服务端：

```bash
stock-cache serve --database runtime/stock-cache.duckdb
# 监听网络接口：
stock-cache serve --bind 0.0.0.0:9494
```

`serve` 将 DuckDB 原生的 **Quack** 协议封装在同一个可执行程序内。它以只读方式打开已有数据库，默认监听 `127.0.0.1:9494`。可在环境变量或 `.env` 中设置 `QUACK_BIND` 和 `QUACK_AUTH_TOKEN`，或用 `--bind` 和 `--auth-token` 覆盖。无论绑定到哪个地址都必须提供 token，以免 DuckDB 静默生成一个无法访问的凭据。启动时的 JSON 会报告就绪状态、传输 URL 和 TLS 证书指纹，但不会打印 token。

在支持 Quack 的 DuckDB 客户端中，挂载服务端并查询其表：

```sql
ATTACH 'quack:server-host:9494' AS market
  (TOKEN 'your-token', SSL_FINGERPRINT 'fingerprint-from-startup-json');
SELECT count(*) FROM market.main.calc_daily_indicators;
DETACH market;
```

Quack 在 localhost 上使用 HTTP，在绑定网络接口时使用 HTTPS 并自动生成证书。远程客户端使用当前启动 JSON 中的 `ssl_fingerprint`；连接默认的本地 HTTP 服务时可省略 `SSL_FINGERPRINT`。在客户端中请将通配符监听地址替换为服务端可访问的主机名。Quack 以流式方式返回 SQL 查询结果，并不会复制 `.duckdb` 文件。Ctrl-C 或 SIGTERM 会停止服务并释放数据库锁。在运行本地写入前请先停止服务，更新完成后再重新启动。systemd 或 Docker 等进程管理器可以直接托管该命令；无需额外的启动脚本或 DuckDB 可执行文件。

## 写入策略

- 每日追加：对新逻辑键范围使用 `append`。
- 历史补齐或修正某个日期：使用带显式日期集合的 `replace-dates`。
- 显式删除或替换行：使用带完整逻辑键的 `replace-keys`。

写入器将 JSONL 批次载入 DuckDB 暂存表，按逻辑键去重，应用日期/键替换，提交一次并写入检查点。它不执行生产中逐行 upsert 的操作。

## 验证

```bash
cargo fmt --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
git diff --check
```

有关计算语义、预热规则、实测数据采集边界和消费端 SQL，请参阅设计与数据使用文档。

写入器基准测试使用开发者示例，而非产品 CLI：

```bash
cargo build --release --example writer-benchmark
python3 tools/benchmark_rust_writer.py
```
