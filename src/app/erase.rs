//! Atomic, date-bounded erasure of market data; reference data and calendar survive.
use std::{collections::BTreeMap, path::PathBuf};

use anyhow::{ensure, Result};
use chrono::NaiveDate;
use duckdb_neo::Parameters;
use serde_json::json;
use stock_cache::{metadata::tushare_fields::DATASETS, storage::duckdb};

pub(crate) struct Args {
    pub(crate) database: Option<String>,
    pub(crate) start_date: String,
    pub(crate) end_date: String,
}

fn parse_date(text: &str) -> Result<NaiveDate> {
    ensure!(
        text.len() == 10 && text.as_bytes()[4] == b'-' && text.as_bytes()[7] == b'-',
        "erase dates must use YYYY-MM-DD"
    );
    Ok(NaiveDate::parse_from_str(text, "%Y-%m-%d")?)
}

pub(crate) fn run(args: Args) -> Result<()> {
    let start = parse_date(&args.start_date)?;
    let end = parse_date(&args.end_date)?;
    ensure!(start <= end, "--start-date must be <= --end-date");
    let database = args
        .database
        .or_else(|| std::env::var("DUCKDB_PATH").ok())
        .unwrap_or_else(|| "runtime/stock-cache.duckdb".into());
    ensure!(
        PathBuf::from(&database).is_file(),
        "database does not exist: {database}"
    );
    let counts = duckdb::with_connection(&database, |connection| {
        duckdb::create_schema(connection)?;
        let mut targets = DATASETS
            .iter()
            .filter(|d| d.field("trade_date").is_some())
            .map(|d| (d.table, "trade_date"))
            .collect::<Vec<_>>();
        targets.extend([
            ("source_daily_bars", "trade_date"),
            ("calc_period_bars", "period_end_date"),
            ("calc_indicator_state", "period_end_date"),
            ("calc_daily_indicators", "trade_date"),
            ("calc_daily_czsc_structure", "trade_date"),
            ("calc_daily_rolling_factors", "trade_date"),
            ("calc_daily_left_peak", "trade_date"),
            ("cache_fetch_coverage", "trade_date"),
        ]);
        connection.execute("BEGIN TRANSACTION", Parameters::None)?;
        let result = (|| -> Result<BTreeMap<String, i64>> {
            let mut counts = BTreeMap::new();
            for (table, column) in targets {
                // Identifiers come exclusively from metadata and the fixed list above.
                let predicate = format!("\"{column}\" BETWEEN DATE '{start}' AND DATE '{end}'");
                let mut query = connection.query(
                    format!("SELECT count(*)::BIGINT FROM \"{table}\" WHERE {predicate}"),
                    Parameters::None,
                )?;
                let chunk = query.next_chunk()?.expect("count returns one row");
                let count = chunk
                    .get_vector_at::<i64>(0)?
                    .reader()?
                    .get(0)?
                    .copied()
                    .unwrap_or(0);
                drop(query);
                connection.execute(
                    format!("DELETE FROM \"{table}\" WHERE {predicate}"),
                    Parameters::None,
                )?;
                counts.insert(table.to_string(), count);
            }
            connection.execute("COMMIT", Parameters::None)?;
            Ok(counts)
        })();
        if result.is_err() {
            let _ = connection.execute("ROLLBACK", Parameters::None);
        }
        let counts = result?;
        duckdb::checkpoint(connection)?;
        Ok(counts)
    })?;
    println!(
        "{}",
        json!({"status":"success", "mode":"erase", "database":database, "start_date":start, "end_date":end, "deleted_row_counts":counts})
    );
    Ok(())
}
