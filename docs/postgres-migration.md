# 从旧 PostgreSQL 一次性迁移

`examples/import-postgres.rs` 是开发辅助工具，不属于 `stock-cache` 产品子命令。它把旧库中的 Tushare **源表和参考表**导入 DuckDB，并重建 `cache_fetch_coverage`，让后续 `write --mode full` 不再重复拉取已迁移的历史。

## 前提

- 旧 PostgreSQL 中保存的是与 Tushare 接口一致的源表（例如 v2 的 `stock_stk_factor_pro`、`index_idx_factor_pro`、`index_ths_daily` 等）。
- 首次运行需要访问 `extensions.duckdb.org` 下载 DuckDB 的 `postgres` 扩展。
- 目标 DuckDB 文件没有被 `serve` 或其他写入进程占用。

## 用法

```bash
cargo run --release --example import-postgres -- \
  --dsn 'postgresql://user:password@host:5432/db' \
  --database runtime/stock-cache.duckdb
```

也可以使用环境变量 `STOCK_CACHE_PG_DSN` 提供 DSN；`--database` 缺省时读取 `DUCKDB_PATH`，再缺省为 `runtime/stock-cache.duckdb`。

常用选项：

| 选项 | 作用 |
| --- | --- |
| `--dry-run` | 只列出 PostgreSQL 中可读的表、行数和匹配列，不写入 |
| `--tables a,b` | 只导入指定数据集 |
| `--since YYYY-MM-DD` | 带日期表的起始日（含） |
| `--until YYYY-MM-DD` | 带日期表的结束日（含）；缺省时取当前 DuckDB 的最新日期 |
| `--keep-off-calendar` | 保留不在交易日历中的日期行（默认在导入后删除） |
| `--insert-only` | 不删除、不覆盖已有行；只补齐目标库缺少的键 |
| `--skip-coverage` | 不重建 `cache_fetch_coverage` |
| `--calendar-only` | 不连接 PostgreSQL，只从现有 DuckDB 表重建交易日历和覆盖记录 |

只迁某个区间时，`--until` 可以省略，工具会读取目标 DuckDB 中已有的最新日期：

```bash
cargo run --release --example import-postgres -- \
  --dsn 'postgresql://user:password@host:5432/db' \
  --since 2024-01-01 \
  --database runtime/stock-cache.duckdb
```

该区间内 DuckDB 已有的行会被 PostgreSQL 数据替换。

如果只想补齐缺失数据、不覆盖已有行，加 `--insert-only`：

```bash
cargo run --release --example import-postgres -- \
  --dsn 'postgresql://user:password@host:5432/db' \
  --since 2024-01-01 --insert-only \
  --database runtime/stock-cache.duckdb
```

`--insert-only` 使用 `INSERT OR IGNORE`：已存在的主键行保持原值，只插入缺少的行；参考表也只补充新键，不删除旧行。它同时跳过非交易日清理，因此不会删除任何已有数据。

## 行为

1. 在 DuckDB 中执行 `create_schema`，加载 `postgres` 扩展，并按只读方式挂载 PostgreSQL。
2. 遍历 `DATASETS`，导入 PostgreSQL 中存在的源表/参考表；列按名称取交集，缺失的可空列写入 `NULL`，缺失的必填列直接报错。
3. 目标表为空时使用普通 `INSERT`；已有数据时先删除区间再插入，因此可以重复执行。带 `trade_date` 的表按年份分批导入，避免单事务过大。`--insert-only` 改为 `INSERT OR IGNORE`，不执行任何删除。
4. `--since`/`--until` 只作用于带 `trade_date` 的表；参考表仍整体导入。导入完成后，默认删除区间内 `cache_trade_calendar` 中不存在的日期，避免旧库中的非交易日 bar 让后续计算失败；日历范围之外的日期会保留。需要保留这些行时用 `--keep-off-calendar`；`--insert-only` 也会跳过这一步。
5. 参考表（`stock_basic`、`index_ths`、`index_ths_member`）按 `ReplaceAll` 语义先清空再写入。
6. 从导入的带日期表重建 `cache_trade_calendar`。`write` 只抓取最近约 38 个月的日历，更早的迁移数据必须靠这一步补齐，否则计算会报 `bar date ... is absent from trading calendar`。
7. 从导入后的 DuckDB 表重建 `cache_fetch_coverage`。带日期的表会为区间内每个交易日生成覆盖记录（没有数据的日期记为 0 行）；`index_idx_factor_pro` 还会对 `INDEX_LIST_PATH`（默认 `config/default-indexes.csv`）中的每个指数代码生成覆盖，包括指数上市前的日期。否则像 `000680.SH` 这种 2025 年才上市的指数会让 36 个月预热检查失败。
8. **不导入任何 `calc_*` 计算结果**。计算版本可能与旧库不同，迁移后由 Rust 实现重新计算。

## 迁移之后

只算最近两个交易日：

```bash
target/release/stock-cache write --mode full --lookback-trading-days 2
```

重算迁移区间内的计算结果（不访问 Tushare）：

```bash
target/release/stock-cache write --mode calc \
  --start-date 2024-01-01 --end-date 2026-11-18
```

注意：计算需要目标区间之前 **36 个月**的 `CALC_SOURCES` 覆盖记录。最早可补算的日期 = 导入起点 + 36 个月：

| 导入起点 | 最早可补算日期（约） |
| --- | --- |
| `--since 2021-01-01` | **2024-01-01**（首个交易日 2024-01-02） |
| `--since 2024-01-01` | 2027-01-01 之后，实际不可用 |

如果只迁移了 2024-01-01 之后的数据却要从 2024-01-01 开始重算，预热区间（2021-01 至 2023-12）会缺失并报错。此时应把导入起点提前到 2021-01-01，或者只重算预热足够的新近日期。

## 注意事项

- **qfq 数据的新鲜度**：股票使用前复权价。旧快照越久，整段历史的 qfq 可能与当前 Tushare 不一致，影响 MACD 种子和滚动因子。必要时只迁移原始字段，或让 `write` 重新拉取 `stock_stk_factor_pro`。
- **覆盖表**：如果使用 `--skip-coverage`，`write` 会认为历史未覆盖并重新抓取，迁移就失去意义。
- **版本**：本项目的 DuckDB 绑定固定为 1.2.0。不要用更高版本的 DuckDB 客户端写入同一个文件，否则可能升级存储格式导致本工具无法打开。
- **失败重跑**：导入是幂等的；中断后直接重新执行即可。如果数据已经导入，只是日历或覆盖记录不对，用 `--calendar-only` 快速修复，不必重新传输数据：

```bash
cargo run --release --example import-postgres -- \
  --calendar-only --database runtime/stock-cache.duckdb
```
