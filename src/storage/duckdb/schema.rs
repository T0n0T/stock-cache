//! DuckDB schema, connection lifecycle, and table metadata.

use anyhow::{anyhow, bail, Context, Result};
use chrono::NaiveDate;
use duckdb_neo::{
    connection::Connection,
    environment::{Environment, StorageLocation},
    Parameters,
};

use crate::metadata::tushare_fields::{dataset, WriteStrategy, DATASETS};

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
/// Local derivation of stock moneyflow into SW2021 level-2 industry totals.
pub const INDUSTRY_MONEYFLOW_TABLE: &str = "calc_daily_industry_moneyflow";
pub const INDUSTRY_MONEYFLOW_VERSION: &str = "sw2021-l2-v1";
const INDUSTRY_MONEYFLOW_COLUMNS: &[&str] = &[
    "source_dataset",
    "industry_code",
    "industry_name",
    "trade_date",
    "stock_count",
    "buy_sm_amount",
    "sell_sm_amount",
    "net_sm_amount",
    "buy_md_amount",
    "sell_md_amount",
    "net_md_amount",
    "buy_lg_amount",
    "sell_lg_amount",
    "net_lg_amount",
    "buy_elg_amount",
    "sell_elg_amount",
    "net_elg_amount",
    "main_net_amount",
    "net_mf_vol",
    "net_mf_amount",
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
        "calc_period_bars" => PERIOD_COLUMNS.iter().map(|x| (*x).to_string()).collect(),
        "calc_indicator_state" => STATE_COLUMNS.iter().map(|x| (*x).to_string()).collect(),
        "calc_daily_indicators" => INDICATOR_COLUMNS.iter().map(|x| (*x).to_string()).collect(),
        "calc_daily_rolling_factors" => ROLLING_COLUMNS.iter().map(|x| (*x).to_string()).collect(),
        "calc_daily_left_peak" => LEFT_PEAK_COLUMNS.iter().map(|x| (*x).to_string()).collect(),
        "calc_daily_industry_moneyflow" => INDUSTRY_MONEYFLOW_COLUMNS
            .iter()
            .map(|x| (*x).to_string())
            .collect(),
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
            .ok_or_else(|| anyhow!("unknown cache table: {table}"))?,
    };
    Ok(result)
}

pub fn table_keys(table: &str) -> Result<Vec<String>> {
    Ok(match table {
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
        "calc_daily_industry_moneyflow" => {
            vec![
                "source_dataset",
                "industry_code",
                "trade_date",
                "calc_version",
            ]
        }
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
            None => bail!("unknown cache table: {table}"),
        },
    }
    .into_iter()
    .map(String::from)
    .collect())
}

pub(super) fn ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}
pub(super) fn sql_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
pub(super) fn date_literal(value: &str) -> Result<String> {
    Ok(format!(
        "DATE {}",
        sql_string(&parse_date(value)?.to_string())
    ))
}
pub(super) fn parse_date(value: &str) -> Result<NaiveDate> {
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
            | "industry_code"
            | "industry_name"
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

pub(super) fn type_for_table_column(table: &str, column: &str) -> &'static str {
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

pub(super) fn execute_script(connection: &Connection, sql: &str) -> Result<()> {
    for statement in connection.parse(sql)? {
        connection.execute(statement?, Parameters::None)?;
    }
    Ok(())
}

pub fn create_schema(connection: &Connection) -> Result<()> {
    connection.execute("DROP TABLE IF EXISTS source_daily_bars", Parameters::None)?;
    execute_script(connection, SCHEMA_SQL)?;
    execute_script(connection, &source_schema_sql())
}

pub fn checkpoint(connection: &Connection) -> Result<()> {
    connection
        .execute("CHECKPOINT", Parameters::None)
        .map(|_| ())
        .map_err(Into::into)
}

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS cache_trade_calendar (trade_date DATE PRIMARY KEY);
CREATE TABLE IF NOT EXISTS cache_fetch_coverage (dataset VARCHAR, trade_date DATE, code VARCHAR, row_count BIGINT NOT NULL CHECK (row_count >= 0), PRIMARY KEY(dataset, trade_date, code));
CREATE TABLE IF NOT EXISTS calc_period_bars (source_dataset VARCHAR NOT NULL, ts_code VARCHAR NOT NULL, period_type VARCHAR NOT NULL, period_start_date DATE NOT NULL, period_end_date DATE NOT NULL, price_basis VARCHAR NOT NULL, open DOUBLE, high DOUBLE, low DOUBLE, close DOUBLE, vol DOUBLE, amount DOUBLE, calc_version VARCHAR NOT NULL, PRIMARY KEY (source_dataset, ts_code, period_type, period_end_date, calc_version));
CREATE TABLE IF NOT EXISTS calc_indicator_state (source_dataset VARCHAR NOT NULL, ts_code VARCHAR NOT NULL, period_type VARCHAR NOT NULL, period_start_date DATE NOT NULL, period_end_date DATE NOT NULL, macd_ema12 DOUBLE, macd_ema26 DOUBLE, macd_dea DOUBLE, macd_period_count BIGINT, kdj_k DOUBLE, kdj_d DOUBLE, kdj_period_count BIGINT, calc_version VARCHAR NOT NULL, PRIMARY KEY (source_dataset, ts_code, period_type, period_end_date, calc_version));
CREATE TABLE IF NOT EXISTS calc_daily_indicators (source_dataset VARCHAR NOT NULL, ts_code VARCHAR NOT NULL, trade_date DATE NOT NULL, daily_macd_dif_asof DOUBLE, daily_macd_dea_asof DOUBLE, daily_macd_hist_x2_asof DOUBLE, daily_macd_dea_pctile_asof DOUBLE, daily_macd_period_count BIGINT CHECK (daily_macd_period_count IS NULL OR daily_macd_period_count >= 0), weekly_macd_dif_asof DOUBLE, weekly_macd_dea_asof DOUBLE, weekly_macd_hist_x2_asof DOUBLE, weekly_macd_dea_pctile_asof DOUBLE, weekly_macd_period_count BIGINT, monthly_macd_dif_asof DOUBLE, monthly_macd_dea_asof DOUBLE, monthly_macd_hist_x2_asof DOUBLE, monthly_macd_dea_pctile_asof DOUBLE, monthly_macd_period_count BIGINT, weekly_kdj_k_asof DOUBLE, weekly_kdj_d_asof DOUBLE, weekly_kdj_j_asof DOUBLE, monthly_kdj_k_asof DOUBLE, monthly_kdj_d_asof DOUBLE, monthly_kdj_j_asof DOUBLE, calc_version VARCHAR NOT NULL, PRIMARY KEY (source_dataset, ts_code, trade_date, calc_version));
CREATE TABLE IF NOT EXISTS calc_daily_czsc_structure (source_dataset VARCHAR NOT NULL, ts_code VARCHAR NOT NULL, trade_date DATE NOT NULL, daily_period_complete BOOLEAN, weekly_period_complete BOOLEAN, monthly_period_complete BOOLEAN, daily_source_bar_valid BOOLEAN, daily_status VARCHAR, daily_available_finished_bi_count INTEGER, daily_recent_bi_count INTEGER, daily_last_bi_direction VARCHAR, daily_last_bi_start_date DATE, daily_last_bi_end_date DATE, daily_last_bi_high DOUBLE, daily_last_bi_low DOUBLE, daily_last_bi_mid DOUBLE, daily_latest_zs_start_date DATE, daily_latest_zs_end_date DATE, daily_latest_zs_bi_count INTEGER, daily_latest_zs_zg DOUBLE, daily_latest_zs_zd DOUBLE, daily_latest_zs_zz DOUBLE, daily_recent_peak_date DATE, daily_recent_peak_high DOUBLE, daily_post_peak_first_bear_date DATE, daily_post_peak_first_bear_open DOUBLE, daily_post_peak_first_bear_high DOUBLE, daily_post_peak_first_bear_low DOUBLE, daily_post_peak_first_bear_close DOUBLE, daily_post_peak_first_bear_missing BOOLEAN, weekly_source_bar_valid BOOLEAN, weekly_status VARCHAR, weekly_available_finished_bi_count INTEGER, weekly_recent_bi_count INTEGER, weekly_last_bi_direction VARCHAR, weekly_last_bi_start_date DATE, weekly_last_bi_end_date DATE, weekly_last_bi_high DOUBLE, weekly_last_bi_low DOUBLE, weekly_last_bi_mid DOUBLE, weekly_latest_zs_start_date DATE, weekly_latest_zs_end_date DATE, weekly_latest_zs_bi_count INTEGER, weekly_latest_zs_zg DOUBLE, weekly_latest_zs_zd DOUBLE, weekly_latest_zs_zz DOUBLE, weekly_recent_peak_date DATE, weekly_recent_peak_high DOUBLE, weekly_post_peak_first_bear_date DATE, weekly_post_peak_first_bear_open DOUBLE, weekly_post_peak_first_bear_high DOUBLE, weekly_post_peak_first_bear_low DOUBLE, weekly_post_peak_first_bear_close DOUBLE, weekly_post_peak_first_bear_missing BOOLEAN, monthly_source_bar_valid BOOLEAN, monthly_status VARCHAR, monthly_available_finished_bi_count INTEGER, monthly_recent_bi_count INTEGER, monthly_last_bi_direction VARCHAR, monthly_last_bi_start_date DATE, monthly_last_bi_end_date DATE, monthly_last_bi_high DOUBLE, monthly_last_bi_low DOUBLE, monthly_last_bi_mid DOUBLE, monthly_latest_zs_start_date DATE, monthly_latest_zs_end_date DATE, monthly_latest_zs_bi_count INTEGER, monthly_latest_zs_zg DOUBLE, monthly_latest_zs_zd DOUBLE, monthly_latest_zs_zz DOUBLE, monthly_recent_peak_date DATE, monthly_recent_peak_high DOUBLE, monthly_post_peak_first_bear_date DATE, monthly_post_peak_first_bear_open DOUBLE, monthly_post_peak_first_bear_high DOUBLE, monthly_post_peak_first_bear_low DOUBLE, monthly_post_peak_first_bear_close DOUBLE, monthly_post_peak_first_bear_missing BOOLEAN, calc_version VARCHAR NOT NULL, PRIMARY KEY (source_dataset, ts_code, trade_date, calc_version));
CREATE TABLE IF NOT EXISTS calc_daily_rolling_factors (source_dataset VARCHAR NOT NULL, ts_code VARCHAR NOT NULL, trade_date DATE NOT NULL, ma25_qfq DOUBLE, ma144_qfq DOUBLE, ma220_qfq DOUBLE, high_20_qfq DOUBLE, high_90_qfq DOUBLE, low_90_qfq DOUBLE, high_120_qfq DOUBLE, low_120_qfq DOUBLE, close_to_20d_max_close_pct DOUBLE, position_90d DOUBLE, position_120d DOUBLE, volume_ma5 DOUBLE, volume_ma20 DOUBLE, volume_to_ma5_ratio DOUBLE, volume_to_ma20_ratio DOUBLE, volume_ma5_to_ma20_ratio DOUBLE, turnover_rate_ma5 DOUBLE, turnover_to_ma5_ratio DOUBLE, range_compression_20d DOUBLE, range_compression_40d DOUBLE, calc_version VARCHAR NOT NULL, PRIMARY KEY (source_dataset, ts_code, trade_date, calc_version));
CREATE TABLE IF NOT EXISTS calc_daily_left_peak (source_dataset VARCHAR NOT NULL, ts_code VARCHAR NOT NULL, trade_date DATE NOT NULL, left_peak_date DATE, left_peak_high DOUBLE, breakout_date DATE, breakout_close DOUBLE, breakout_body_above_left_peak BOOLEAN, first_bear_date DATE, first_bear_open DOUBLE, first_bear_missing BOOLEAN, pick_close DOUBLE, b_div_a DOUBLE, abs_ba_minus_1 DOUBLE, a_lt_b BOOLEAN, is_valid BOOLEAN, status VARCHAR, calc_version VARCHAR NOT NULL, PRIMARY KEY (source_dataset, ts_code, trade_date, calc_version));
CREATE TABLE IF NOT EXISTS calc_daily_industry_moneyflow (source_dataset VARCHAR NOT NULL, industry_code VARCHAR NOT NULL, industry_name VARCHAR, trade_date DATE NOT NULL, stock_count BIGINT, buy_sm_amount DOUBLE, sell_sm_amount DOUBLE, net_sm_amount DOUBLE, buy_md_amount DOUBLE, sell_md_amount DOUBLE, net_md_amount DOUBLE, buy_lg_amount DOUBLE, sell_lg_amount DOUBLE, net_lg_amount DOUBLE, buy_elg_amount DOUBLE, sell_elg_amount DOUBLE, net_elg_amount DOUBLE, main_net_amount DOUBLE, net_mf_vol DOUBLE, net_mf_amount DOUBLE, calc_version VARCHAR NOT NULL, PRIMARY KEY (source_dataset, industry_code, trade_date, calc_version));
"#;
