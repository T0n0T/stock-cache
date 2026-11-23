use std::{fs, path::Path};

use anyhow::Result;
use serde_json::Value;
use stock_cache::storage::duckdb;

pub(crate) fn init(database: Option<String>) -> Result<()> {
    let database = prepare_database(database)?;
    duckdb::with_connection(&database, |connection| {
        duckdb::create_schema(connection)?;
        duckdb::checkpoint(connection)?;
        println!(
            "{{\"status\":\"ok\",\"database\":{}}}",
            serde_json::to_string(&database)?
        );
        Ok(())
    })
}

pub(crate) fn status(database: Option<String>) -> Result<()> {
    let database = prepare_database(database)?;
    duckdb::with_connection(&database, |connection| {
        duckdb::create_schema(connection)?;
        let tables = [
            "cache_trade_calendar",
            "cache_fetch_coverage",
            "stock_basic",
            "stock_stk_factor_pro",
            "stock_moneyflow",
            "index_idx_factor_pro",
            "index_ths",
            "index_ths_daily",
            "index_ths_member",
            "index_sw_classify",
            "index_sw_member",
            "index_weight",
            "index_limit_cpt_list",
            "stock_limit_list_ths",
            "calc_period_bars",
            "calc_indicator_state",
            "calc_daily_indicators",
            "calc_daily_czsc_structure",
            "calc_daily_rolling_factors",
            "calc_daily_left_peak",
            "calc_daily_industry_moneyflow",
        ];
        let mut counts = serde_json::Map::new();
        for table in tables {
            counts.insert(
                table.to_string(),
                Value::from(duckdb::table_count(connection, table)?),
            );
        }
        println!(
            "{}",
            serde_json::json!({"database": database, "tables": counts})
        );
        Ok(())
    })
}

pub(crate) fn resolve_database(database: Option<String>) -> String {
    database
        .or_else(|| std::env::var("DUCKDB_PATH").ok())
        .unwrap_or_else(|| "runtime/stock-cache.duckdb".into())
}

fn prepare_database(database: Option<String>) -> Result<String> {
    let database = resolve_database(database);
    if let Some(parent) = Path::new(&database)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    Ok(database)
}
