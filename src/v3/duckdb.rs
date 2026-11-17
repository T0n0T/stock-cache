//! Rust-owned DuckDB schema and batch writer for v3.
//!
//! Input is JSONL only at this boundary. DuckDB reads the JSONL file into a temporary
//! staging table, then all validation, replacement, deduplication and commit work is
//! executed as set-based SQL on one v2-C-API connection.

use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{anyhow, bail, Context, Result};
use chrono::NaiveDate;
use duckdb_neo::{
    connection::Connection,
    environment::{Environment, StorageLocation},
    Parameters,
};
use serde_json::Value;

use super::checkpoint::IndicatorCheckpoint;
use crate::metadata::tushare_fields::{dataset, WriteStrategy, DATASETS};

const SOURCE_COLUMNS: &[&str] = &[
    "source_dataset",
    "ts_code",
    "trade_date",
    "open",
    "high",
    "low",
    "close",
    "vol",
    "amount",
    "turnover_rate",
    "price_basis",
];
const PERIOD_COLUMNS: &[&str] = &[
    "source_dataset",
    "ts_code",
    "period_type",
    "period_start_date",
    "period_end_date",
    "price_basis",
    "open",
    "high",
    "low",
    "close",
    "vol",
    "amount",
    "calc_version",
];
const STATE_COLUMNS: &[&str] = &[
    "source_dataset",
    "ts_code",
    "period_type",
    "period_start_date",
    "period_end_date",
    "macd_ema12",
    "macd_ema26",
    "macd_dea",
    "macd_period_count",
    "kdj_k",
    "kdj_d",
    "kdj_period_count",
    "calc_version",
];
const INDICATOR_COLUMNS: &[&str] = &[
    "source_dataset",
    "ts_code",
    "trade_date",
    "daily_macd_dif_asof",
    "daily_macd_dea_asof",
    "daily_macd_hist_x2_asof",
    "daily_macd_dea_pctile_asof",
    "daily_macd_period_count",
    "weekly_macd_dif_asof",
    "weekly_macd_dea_asof",
    "weekly_macd_hist_x2_asof",
    "weekly_macd_dea_pctile_asof",
    "weekly_macd_period_count",
    "monthly_macd_dif_asof",
    "monthly_macd_dea_asof",
    "monthly_macd_hist_x2_asof",
    "monthly_macd_dea_pctile_asof",
    "monthly_macd_period_count",
    "weekly_kdj_k_asof",
    "weekly_kdj_d_asof",
    "weekly_kdj_j_asof",
    "monthly_kdj_k_asof",
    "monthly_kdj_d_asof",
    "monthly_kdj_j_asof",
    "calc_version",
];
const ROLLING_COLUMNS: &[&str] = &[
    "source_dataset",
    "ts_code",
    "trade_date",
    "ma25_qfq",
    "ma144_qfq",
    "ma220_qfq",
    "high_20_qfq",
    "high_90_qfq",
    "low_90_qfq",
    "high_120_qfq",
    "low_120_qfq",
    "close_to_20d_max_close_pct",
    "position_90d",
    "position_120d",
    "volume_ma5",
    "volume_ma20",
    "volume_to_ma5_ratio",
    "volume_to_ma20_ratio",
    "volume_ma5_to_ma20_ratio",
    "turnover_rate_ma5",
    "turnover_to_ma5_ratio",
    "range_compression_20d",
    "range_compression_40d",
    "calc_version",
];
const LEFT_PEAK_COLUMNS: &[&str] = &[
    "source_dataset",
    "ts_code",
    "trade_date",
    "left_peak_date",
    "left_peak_high",
    "breakout_date",
    "breakout_close",
    "breakout_body_above_left_peak",
    "first_bear_date",
    "first_bear_open",
    "first_bear_missing",
    "pick_close",
    "b_div_a",
    "abs_ba_minus_1",
    "a_lt_b",
    "is_valid",
    "status",
    "calc_version",
];

fn czsc_columns(prefix: &str) -> Vec<String> {
    [
        "source_bar_valid",
        "status",
        "available_finished_bi_count",
        "recent_bi_count",
        "last_bi_direction",
        "last_bi_start_date",
        "last_bi_end_date",
        "last_bi_high",
        "last_bi_low",
        "last_bi_mid",
        "latest_zs_start_date",
        "latest_zs_end_date",
        "latest_zs_bi_count",
        "latest_zs_zg",
        "latest_zs_zd",
        "latest_zs_zz",
        "recent_peak_date",
        "recent_peak_high",
        "post_peak_first_bear_date",
        "post_peak_first_bear_open",
        "post_peak_first_bear_high",
        "post_peak_first_bear_low",
        "post_peak_first_bear_close",
        "post_peak_first_bear_missing",
    ]
    .into_iter()
    .map(|name| format!("{prefix}_{name}"))
    .collect()
}

pub fn table_columns(table: &str) -> Result<Vec<String>> {
    let result = match table {
        "source_daily_bars" => SOURCE_COLUMNS.iter().map(|x| (*x).to_string()).collect(),
        "calc_period_bars" => PERIOD_COLUMNS.iter().map(|x| (*x).to_string()).collect(),
        "calc_indicator_state" => STATE_COLUMNS.iter().map(|x| (*x).to_string()).collect(),
        "calc_daily_indicators" => INDICATOR_COLUMNS.iter().map(|x| (*x).to_string()).collect(),
        "calc_daily_rolling_factors" => ROLLING_COLUMNS.iter().map(|x| (*x).to_string()).collect(),
        "calc_daily_left_peak" => LEFT_PEAK_COLUMNS.iter().map(|x| (*x).to_string()).collect(),
        "calc_daily_czsc_structure" => {
            let mut values = vec![
                "source_dataset",
                "ts_code",
                "trade_date",
                "daily_period_complete",
                "weekly_period_complete",
                "monthly_period_complete",
            ]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>();
            values.extend(czsc_columns("daily"));
            values.extend(czsc_columns("weekly"));
            values.extend(czsc_columns("monthly"));
            values.push("calc_version".to_string());
            values
        }
        _ => dataset(table)
            .map(|metadata| {
                metadata
                    .fields
                    .iter()
                    .map(|field| field.name.to_string())
                    .collect()
            })
            .ok_or_else(|| anyhow!("unknown v3 table: {table}"))?,
    };
    Ok(result)
}

pub fn table_keys(table: &str) -> Result<Vec<String>> {
    Ok(match table {
        "source_daily_bars" => vec!["source_dataset", "ts_code", "trade_date"],
        "calc_period_bars" => vec![
            "source_dataset",
            "ts_code",
            "period_type",
            "period_end_date",
            "calc_version",
        ],
        "calc_indicator_state" => vec![
            "source_dataset",
            "ts_code",
            "period_type",
            "period_end_date",
            "calc_version",
        ],
        "calc_daily_indicators"
        | "calc_daily_czsc_structure"
        | "calc_daily_rolling_factors"
        | "calc_daily_left_peak" => vec!["source_dataset", "ts_code", "trade_date", "calc_version"],
        _ => match dataset(table) {
            Some(metadata) => match metadata.write_strategy {
                WriteStrategy::Upsert { key } | WriteStrategy::ReplaceAll { key, .. } => {
                    key.to_vec()
                }
                WriteStrategy::DeleteInsertByDate {
                    natural_key_candidate,
                    ..
                } => natural_key_candidate.to_vec(),
            },
            None => bail!("unknown v3 table: {table}"),
        },
    }
    .into_iter()
    .map(String::from)
    .collect())
}

fn ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}
fn sql_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
fn date_literal(value: &str) -> Result<String> {
    Ok(format!(
        "DATE {}",
        sql_string(&parse_date(value)?.to_string())
    ))
}
fn parse_date(value: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(value, "%Y%m%d"))
        .map_err(|_| anyhow!("invalid date: {value}"))
}

fn type_for(column: &str) -> &'static str {
    if column.ends_with("_date") || column == "trade_date" {
        "DATE"
    } else if column.ends_with("_count") || column.ends_with("_bi_count") {
        "BIGINT"
    } else if column.ends_with("_valid")
        || column.ends_with("_complete")
        || column.ends_with("_missing")
        || column == "a_lt_b"
        || column == "is_valid"
        || column == "breakout_body_above_left_peak"
    {
        "BOOLEAN"
    } else if matches!(
        column,
        "source_dataset"
            | "ts_code"
            | "period_type"
            | "price_basis"
            | "calc_version"
            | "status"
            | "daily_last_bi_direction"
            | "weekly_last_bi_direction"
            | "monthly_last_bi_direction"
    ) || column.ends_with("_direction")
        || column.ends_with("_status")
    {
        "VARCHAR"
    } else {
        "DOUBLE"
    }
}

fn metadata_type_for(table: &str, column: &str) -> Option<&'static str> {
    let field = dataset(table)?.field(column)?;
    Some(field.duckdb_type)
}

fn type_for_table_column(table: &str, column: &str) -> &'static str {
    metadata_type_for(table, column).unwrap_or_else(|| type_for(column))
}

fn source_schema_sql() -> String {
    let mut sql = String::new();
    for metadata in DATASETS {
        let columns = metadata
            .fields
            .iter()
            .map(|field| {
                format!(
                    "{} {}{}",
                    ident(field.name),
                    field.duckdb_type,
                    if field.nullable { "" } else { " NOT NULL" }
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let keys = table_keys(metadata.table).expect("generated source table key");
        let key_sql = keys
            .iter()
            .map(|key| ident(key))
            .collect::<Vec<_>>()
            .join(", ");
        sql.push_str(&format!(
            "CREATE TABLE IF NOT EXISTS {} ({}, PRIMARY KEY ({}));",
            ident(metadata.table),
            columns,
            key_sql
        ));
    }
    sql
}

pub fn with_connection<T>(database: &str, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
    let environment = Environment::new().context("create DuckDB environment")?;
    let instance = environment
        .open(if database == ":memory:" {
            StorageLocation::InMemory
        } else {
            StorageLocation::OnDisk(database.to_string())
        })
        .context("open DuckDB database")?;
    let connection = instance.connect().context("connect DuckDB")?;
    f(&connection)
}

fn execute_script(connection: &Connection, sql: &str) -> Result<()> {
    for statement in connection.parse(sql)? {
        connection.execute(statement?, Parameters::None)?;
    }
    Ok(())
}

pub fn create_schema(connection: &Connection) -> Result<()> {
    execute_script(connection, SCHEMA_SQL)?;
    execute_script(connection, &source_schema_sql())
}

pub fn checkpoint(connection: &Connection) -> Result<()> {
    connection
        .execute("CHECKPOINT", Parameters::None)
        .map(|_| ())
        .map_err(Into::into)
}

/// Load all persisted indicator states for one source instrument and calculation version.
///
/// Dates are read as text to keep the DuckDB C API boundary independent of its
/// temporal representation, then parsed with the same date rules as the writer.
pub fn load_indicator_checkpoints(
    connection: &Connection,
    source_dataset: &str,
    ts_code: &str,
    calc_version: &str,
) -> Result<Vec<IndicatorCheckpoint>> {
    let sql = format!(
        "SELECT source_dataset, ts_code, period_type, period_start_date::VARCHAR, period_end_date::VARCHAR, macd_ema12, macd_ema26, macd_dea, macd_period_count, kdj_k, kdj_d, kdj_period_count, calc_version FROM calc_indicator_state WHERE source_dataset = {} AND ts_code = {} AND calc_version = {} ORDER BY period_end_date, period_type",
        sql_string(source_dataset),
        sql_string(ts_code),
        sql_string(calc_version),
    );
    let mut result = connection.query(sql, Parameters::None)?;
    let mut rows = Vec::new();
    while let Some(chunk) = result.next_chunk()? {
        let source = chunk.get_vector_at::<String>(0)?.reader()?;
        let code = chunk.get_vector_at::<String>(1)?.reader()?;
        let period = chunk.get_vector_at::<String>(2)?.reader()?;
        let start = chunk.get_vector_at::<String>(3)?.reader()?;
        let end = chunk.get_vector_at::<String>(4)?.reader()?;
        let ema12 = chunk.get_vector_at::<f64>(5)?.reader()?;
        let ema26 = chunk.get_vector_at::<f64>(6)?.reader()?;
        let dea = chunk.get_vector_at::<f64>(7)?.reader()?;
        let macd_count = chunk.get_vector_at::<i64>(8)?.reader()?;
        let kdj_k = chunk.get_vector_at::<f64>(9)?.reader()?;
        let kdj_d = chunk.get_vector_at::<f64>(10)?.reader()?;
        let kdj_count = chunk.get_vector_at::<i64>(11)?.reader()?;
        let version = chunk.get_vector_at::<String>(12)?.reader()?;
        for index in 0..chunk.row_count()? {
            let source = source
                .get(index)?
                .ok_or_else(|| anyhow!("checkpoint source_dataset is NULL"))?;
            let code = code
                .get(index)?
                .ok_or_else(|| anyhow!("checkpoint ts_code is NULL"))?;
            let period = period
                .get(index)?
                .ok_or_else(|| anyhow!("checkpoint period_type is NULL"))?;
            let start = start
                .get(index)?
                .ok_or_else(|| anyhow!("checkpoint period_start_date is NULL"))?;
            let end = end
                .get(index)?
                .ok_or_else(|| anyhow!("checkpoint period_end_date is NULL"))?;
            let version = version
                .get(index)?
                .ok_or_else(|| anyhow!("checkpoint calc_version is NULL"))?;
            rows.push(IndicatorCheckpoint {
                source_dataset: source.to_string(),
                ts_code: code.to_string(),
                period_type: period.to_string(),
                period_start_date: parse_date(start)?,
                period_end_date: parse_date(end)?,
                macd_ema12: ema12.get(index)?.copied(),
                macd_ema26: ema26.get(index)?.copied(),
                macd_dea: dea.get(index)?.copied(),
                macd_period_count: macd_count.get(index)?.copied(),
                kdj_k: kdj_k.get(index)?.copied(),
                kdj_d: kdj_d.get(index)?.copied(),
                kdj_period_count: kdj_count.get(index)?.copied(),
                calc_version: version.to_string(),
            });
        }
    }
    Ok(rows)
}

pub fn table_count(connection: &Connection, table: &str) -> Result<i64> {
    let mut result = connection.query(
        format!("SELECT COUNT(*)::BIGINT FROM {}", ident(table)),
        Parameters::None,
    )?;
    let chunk = result
        .next_chunk()?
        .ok_or_else(|| anyhow!("DuckDB returned no count row"))?;
    Ok(*chunk
        .get_vector_at::<i64>(0)?
        .reader()?
        .get(0)?
        .ok_or_else(|| anyhow!("DuckDB count is NULL"))?)
}

pub fn write_jsonl(
    connection: &Connection,
    table: &str,
    input: &Path,
    strategy: &str,
    dates: &[String],
    scope: &BTreeMap<String, String>,
    key_values: &[Vec<Value>],
) -> Result<i64> {
    if !matches!(strategy, "append" | "replace-dates" | "replace-keys") {
        bail!("unknown v3 write strategy: {strategy}");
    }
    if strategy == "replace-keys" && key_values.is_empty() {
        bail!("replace-keys requires at least one logical key");
    }
    let cols = table_columns(table)?;
    let key_cols = table_keys(table)?;
    for column in scope.keys() {
        if !cols.iter().any(|known| known == column) {
            bail!("scope column is not part of {table}: {column}");
        }
    }
    let stage = "__v3_stage";
    execute_script(connection, &format!("DROP TABLE IF EXISTS {stage}; CREATE TEMP TABLE {stage} AS SELECT * FROM {} LIMIT 0; ALTER TABLE {stage} ADD COLUMN __v3_batch_order BIGINT", ident(table)))?;
    let metadata =
        fs::metadata(input).with_context(|| format!("stat JSONL {}", input.display()))?;
    if metadata.len() > 0 {
        let projection = cols
            .iter()
            .map(|column| {
                let source = ident(column);
                let target_type = type_for_table_column(table, column);
                if target_type == "DATE" {
                    format!(
                        "COALESCE(try_strptime(CAST({source} AS VARCHAR), '%Y-%m-%d')::DATE, try_strptime(CAST({source} AS VARCHAR), '%Y%m%d')::DATE) AS {source}"
                    )
                } else {
                    format!("CAST({source} AS {target_type}) AS {source}")
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let path = sql_string(&input.to_string_lossy());
        let sql = format!("INSERT INTO {stage} ({}, __v3_batch_order) SELECT {projection}, ROW_NUMBER() OVER () - 1 FROM read_json_auto({path}, format='newline_delimited', records=true)", cols.iter().map(|x| ident(x)).collect::<Vec<_>>().join(", "));
        connection.execute(sql, Parameters::None)?;
    }
    let result = (|| -> Result<i64> {
        connection.execute("BEGIN TRANSACTION", Parameters::None)?;
        if strategy == "append" {
            let join = key_cols
                .iter()
                .map(|column| format!("target.{0}=staged.{0}", ident(column)))
                .collect::<Vec<_>>()
                .join(" AND ");
            let exists = format!("SELECT 1 FROM {table} target JOIN (SELECT {keys} FROM {stage} QUALIFY ROW_NUMBER() OVER (PARTITION BY {keys} ORDER BY __v3_batch_order DESC)=1) staged ON {join} LIMIT 1", table=ident(table), keys=key_cols.iter().map(|x| ident(x)).collect::<Vec<_>>().join(", "));
            let mut check = connection.query(exists, Parameters::None)?;
            let already_exists = check.next_chunk()?.is_some();
            drop(check);
            if already_exists {
                bail!("append batch contains a logical key already present in target");
            }
        } else {
            let predicate =
                replacement_predicate(table, &cols, dates, scope, key_cols.as_slice(), key_values)?;
            connection.execute(
                format!("DELETE FROM {} WHERE {predicate}", ident(table)),
                Parameters::None,
            )?;
        }
        let key_sql = key_cols
            .iter()
            .map(|x| ident(x))
            .collect::<Vec<_>>()
            .join(", ");
        let count_sql = format!("SELECT COUNT(*)::BIGINT FROM (SELECT ROW_NUMBER() OVER (PARTITION BY {key_sql} ORDER BY __v3_batch_order DESC) rank FROM {stage}) WHERE rank=1");
        let mut count = connection.query(count_sql, Parameters::None)?;
        let written = count
            .next_chunk()?
            .and_then(|chunk| {
                chunk.get_vector_at::<i64>(0).ok().and_then(|v| {
                    v.reader()
                        .ok()
                        .and_then(|r| r.get(0).ok().flatten().copied())
                })
            })
            .unwrap_or(0);
        drop(count);
        let insert = format!("INSERT INTO {} ({}) SELECT {} FROM {stage} QUALIFY ROW_NUMBER() OVER (PARTITION BY {key_sql} ORDER BY __v3_batch_order DESC)=1", ident(table), cols.iter().map(|x| ident(x)).collect::<Vec<_>>().join(", "), cols.iter().map(|x| ident(x)).collect::<Vec<_>>().join(", "));
        connection.execute(insert, Parameters::None)?;
        connection.execute("COMMIT", Parameters::None)?;
        Ok(written)
    })();
    if result.is_err() {
        let _ = connection.execute("ROLLBACK", Parameters::None);
    }
    let _ = connection.execute(format!("DROP TABLE IF EXISTS {stage}"), Parameters::None);
    result
}

fn replacement_predicate(
    table: &str,
    cols: &[String],
    dates: &[String],
    scope: &BTreeMap<String, String>,
    key_cols: &[String],
    key_values: &[Vec<Value>],
) -> Result<String> {
    if !key_values.is_empty() {
        if key_values.iter().any(|key| key_cols.len() != key.len()) {
            bail!("replace key width does not match table key");
        }
        return Ok(key_values
            .iter()
            .map(|key| {
                key.iter()
                    .enumerate()
                    .map(|(index, value)| {
                        format!(
                            "{} = {}",
                            ident(&key_cols[index]),
                            json_literal(value, type_for_table_column(table, &key_cols[index]))
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" AND ")
            })
            .map(|x| format!("({x})"))
            .collect::<Vec<_>>()
            .join(" OR "));
    }
    if dates.is_empty() {
        bail!("replace-dates requires non-empty dates");
    }
    if !cols
        .iter()
        .any(|x| x == "trade_date" || x == "period_end_date")
    {
        bail!("table has no date replacement column");
    }
    let date_column = if cols.iter().any(|x| x == "trade_date") {
        "trade_date"
    } else {
        "period_end_date"
    };
    let date_sql = dates
        .iter()
        .map(|x| date_literal(x))
        .collect::<Result<Vec<_>>>()?
        .join(", ");
    let mut terms = vec![format!("{} IN ({date_sql})", ident(date_column))];
    if table.starts_with("calc_") {
        for required in ["source_dataset", "calc_version"] {
            if !scope.contains_key(required) {
                bail!("v3 calculation replacement requires source_dataset and calc_version scope");
            }
        }
    }
    for (column, value) in scope {
        terms.push(format!(
            "{} IS NOT DISTINCT FROM {}",
            ident(column),
            json_literal(
                &Value::String(value.clone()),
                type_for_table_column(table, column)
            )
        ));
    }
    Ok(terms.join(" AND "))
}

fn json_literal(value: &Value, ty: &str) -> String {
    if value.is_null() {
        "NULL".to_string()
    } else if ty == "DATE" {
        value
            .as_str()
            .and_then(|raw| date_literal(raw).ok())
            .unwrap_or_else(|| "NULL".to_string())
    } else if ty == "BOOLEAN" || ty == "DOUBLE" || ty == "BIGINT" {
        value.to_string()
    } else {
        let text = value
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| value.to_string());
        sql_string(&text)
    }
}

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS source_daily_bars (source_dataset VARCHAR NOT NULL, ts_code VARCHAR NOT NULL, trade_date DATE NOT NULL, open DOUBLE, high DOUBLE, low DOUBLE, close DOUBLE, vol DOUBLE, amount DOUBLE, turnover_rate DOUBLE, price_basis VARCHAR NOT NULL, PRIMARY KEY (source_dataset, ts_code, trade_date));
ALTER TABLE source_daily_bars ADD COLUMN IF NOT EXISTS turnover_rate DOUBLE;
CREATE TABLE IF NOT EXISTS calc_period_bars (source_dataset VARCHAR NOT NULL, ts_code VARCHAR NOT NULL, period_type VARCHAR NOT NULL, period_start_date DATE NOT NULL, period_end_date DATE NOT NULL, price_basis VARCHAR NOT NULL, open DOUBLE, high DOUBLE, low DOUBLE, close DOUBLE, vol DOUBLE, amount DOUBLE, calc_version VARCHAR NOT NULL, PRIMARY KEY (source_dataset, ts_code, period_type, period_end_date, calc_version));
CREATE TABLE IF NOT EXISTS calc_indicator_state (source_dataset VARCHAR NOT NULL, ts_code VARCHAR NOT NULL, period_type VARCHAR NOT NULL, period_start_date DATE NOT NULL, period_end_date DATE NOT NULL, macd_ema12 DOUBLE, macd_ema26 DOUBLE, macd_dea DOUBLE, macd_period_count BIGINT, kdj_k DOUBLE, kdj_d DOUBLE, kdj_period_count BIGINT, calc_version VARCHAR NOT NULL, PRIMARY KEY (source_dataset, ts_code, period_type, period_end_date, calc_version));
CREATE TABLE IF NOT EXISTS calc_daily_indicators (source_dataset VARCHAR NOT NULL, ts_code VARCHAR NOT NULL, trade_date DATE NOT NULL, daily_macd_dif_asof DOUBLE, daily_macd_dea_asof DOUBLE, daily_macd_hist_x2_asof DOUBLE, daily_macd_dea_pctile_asof DOUBLE, daily_macd_period_count BIGINT CHECK (daily_macd_period_count IS NULL OR daily_macd_period_count >= 0), weekly_macd_dif_asof DOUBLE, weekly_macd_dea_asof DOUBLE, weekly_macd_hist_x2_asof DOUBLE, weekly_macd_dea_pctile_asof DOUBLE, weekly_macd_period_count BIGINT, monthly_macd_dif_asof DOUBLE, monthly_macd_dea_asof DOUBLE, monthly_macd_hist_x2_asof DOUBLE, monthly_macd_dea_pctile_asof DOUBLE, monthly_macd_period_count BIGINT, weekly_kdj_k_asof DOUBLE, weekly_kdj_d_asof DOUBLE, weekly_kdj_j_asof DOUBLE, monthly_kdj_k_asof DOUBLE, monthly_kdj_d_asof DOUBLE, monthly_kdj_j_asof DOUBLE, calc_version VARCHAR NOT NULL, PRIMARY KEY (source_dataset, ts_code, trade_date, calc_version));
CREATE TABLE IF NOT EXISTS calc_daily_czsc_structure (source_dataset VARCHAR NOT NULL, ts_code VARCHAR NOT NULL, trade_date DATE NOT NULL, daily_period_complete BOOLEAN, weekly_period_complete BOOLEAN, monthly_period_complete BOOLEAN, daily_source_bar_valid BOOLEAN, daily_status VARCHAR, daily_available_finished_bi_count INTEGER, daily_recent_bi_count INTEGER, daily_last_bi_direction VARCHAR, daily_last_bi_start_date DATE, daily_last_bi_end_date DATE, daily_last_bi_high DOUBLE, daily_last_bi_low DOUBLE, daily_last_bi_mid DOUBLE, daily_latest_zs_start_date DATE, daily_latest_zs_end_date DATE, daily_latest_zs_bi_count INTEGER, daily_latest_zs_zg DOUBLE, daily_latest_zs_zd DOUBLE, daily_latest_zs_zz DOUBLE, daily_recent_peak_date DATE, daily_recent_peak_high DOUBLE, daily_post_peak_first_bear_date DATE, daily_post_peak_first_bear_open DOUBLE, daily_post_peak_first_bear_high DOUBLE, daily_post_peak_first_bear_low DOUBLE, daily_post_peak_first_bear_close DOUBLE, daily_post_peak_first_bear_missing BOOLEAN, weekly_source_bar_valid BOOLEAN, weekly_status VARCHAR, weekly_available_finished_bi_count INTEGER, weekly_recent_bi_count INTEGER, weekly_last_bi_direction VARCHAR, weekly_last_bi_start_date DATE, weekly_last_bi_end_date DATE, weekly_last_bi_high DOUBLE, weekly_last_bi_low DOUBLE, weekly_last_bi_mid DOUBLE, weekly_latest_zs_start_date DATE, weekly_latest_zs_end_date DATE, weekly_latest_zs_bi_count INTEGER, weekly_latest_zs_zg DOUBLE, weekly_latest_zs_zd DOUBLE, weekly_latest_zs_zz DOUBLE, weekly_recent_peak_date DATE, weekly_recent_peak_high DOUBLE, weekly_post_peak_first_bear_date DATE, weekly_post_peak_first_bear_open DOUBLE, weekly_post_peak_first_bear_high DOUBLE, weekly_post_peak_first_bear_low DOUBLE, weekly_post_peak_first_bear_close DOUBLE, weekly_post_peak_first_bear_missing BOOLEAN, monthly_source_bar_valid BOOLEAN, monthly_status VARCHAR, monthly_available_finished_bi_count INTEGER, monthly_recent_bi_count INTEGER, monthly_last_bi_direction VARCHAR, monthly_last_bi_start_date DATE, monthly_last_bi_end_date DATE, monthly_last_bi_high DOUBLE, monthly_last_bi_low DOUBLE, monthly_last_bi_mid DOUBLE, monthly_latest_zs_start_date DATE, monthly_latest_zs_end_date DATE, monthly_latest_zs_bi_count INTEGER, monthly_latest_zs_zg DOUBLE, monthly_latest_zs_zd DOUBLE, monthly_latest_zs_zz DOUBLE, monthly_recent_peak_date DATE, monthly_recent_peak_high DOUBLE, monthly_post_peak_first_bear_date DATE, monthly_post_peak_first_bear_open DOUBLE, monthly_post_peak_first_bear_high DOUBLE, monthly_post_peak_first_bear_low DOUBLE, monthly_post_peak_first_bear_close DOUBLE, monthly_post_peak_first_bear_missing BOOLEAN, calc_version VARCHAR NOT NULL, PRIMARY KEY (source_dataset, ts_code, trade_date, calc_version));
CREATE TABLE IF NOT EXISTS calc_daily_rolling_factors (source_dataset VARCHAR NOT NULL, ts_code VARCHAR NOT NULL, trade_date DATE NOT NULL, ma25_qfq DOUBLE, ma144_qfq DOUBLE, ma220_qfq DOUBLE, high_20_qfq DOUBLE, high_90_qfq DOUBLE, low_90_qfq DOUBLE, high_120_qfq DOUBLE, low_120_qfq DOUBLE, close_to_20d_max_close_pct DOUBLE, position_90d DOUBLE, position_120d DOUBLE, volume_ma5 DOUBLE, volume_ma20 DOUBLE, volume_to_ma5_ratio DOUBLE, volume_to_ma20_ratio DOUBLE, volume_ma5_to_ma20_ratio DOUBLE, turnover_rate_ma5 DOUBLE, turnover_to_ma5_ratio DOUBLE, range_compression_20d DOUBLE, range_compression_40d DOUBLE, calc_version VARCHAR NOT NULL, PRIMARY KEY (source_dataset, ts_code, trade_date, calc_version));
CREATE TABLE IF NOT EXISTS calc_daily_left_peak (source_dataset VARCHAR NOT NULL, ts_code VARCHAR NOT NULL, trade_date DATE NOT NULL, left_peak_date DATE, left_peak_high DOUBLE, breakout_date DATE, breakout_close DOUBLE, breakout_body_above_left_peak BOOLEAN, first_bear_date DATE, first_bear_open DOUBLE, first_bear_missing BOOLEAN, pick_close DOUBLE, b_div_a DOUBLE, abs_ba_minus_1 DOUBLE, a_lt_b BOOLEAN, is_valid BOOLEAN, status VARCHAR, calc_version VARCHAR NOT NULL, PRIMARY KEY (source_dataset, ts_code, trade_date, calc_version));
"#;
