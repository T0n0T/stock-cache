//! Atomic, date-bounded erasure of market data; reference data and calendar survive.
use std::{collections::BTreeMap, path::PathBuf};

use anyhow::{bail, ensure, Result};
use chrono::{FixedOffset, NaiveDate, Utc};
use duckdb_neo::Parameters;
use serde_json::json;
use stock_cache::{metadata::tushare_fields::DATASETS, storage::duckdb};

pub(crate) struct Args {
    pub(crate) database: Option<String>,
    pub(crate) start_date: Option<String>,
    pub(crate) end_date: Option<String>,
    pub(crate) lookback_trading_days: Option<u32>,
}

fn parse_date(text: &str) -> Result<NaiveDate> {
    ensure!(
        text.len() == 10 && text.as_bytes()[4] == b'-' && text.as_bytes()[7] == b'-',
        "erase dates must use YYYY-MM-DD"
    );
    Ok(NaiveDate::parse_from_str(text, "%Y-%m-%d")?)
}

fn cached_sessions(database: &str, through: NaiveDate) -> Result<Vec<NaiveDate>> {
    duckdb::with_connection(database, |connection| {
        duckdb::create_schema(connection)?;
        let mut query = connection.query(
            format!(
                "SELECT trade_date::VARCHAR FROM cache_trade_calendar \
                 WHERE trade_date <= DATE '{through}' ORDER BY trade_date"
            ),
            Parameters::None,
        )?;
        let mut sessions = Vec::new();
        while let Some(chunk) = query.next_chunk()? {
            let column = chunk.get_vector_at::<String>(0)?.reader()?;
            for index in 0..chunk.row_count()? {
                if let Some(text) = column.get(index)? {
                    sessions.push(parse_date(text)?);
                }
            }
        }
        Ok(sessions)
    })
}

fn lookback_window(sessions: &[NaiveDate], days: u32) -> Result<(NaiveDate, NaiveDate)> {
    ensure!(
        !sessions.is_empty(),
        "cached trade calendar has no sessions up to today; run write first"
    );
    ensure!(
        sessions.len() >= days as usize,
        "cached trade calendar has too few trading days for requested lookback; run write first"
    );
    let selected = &sessions[sessions.len() - days as usize..];
    Ok((
        selected[0],
        *selected.last().expect("lookback window is non-empty"),
    ))
}

fn resolve_window(database: &str, args: &Args) -> Result<(NaiveDate, NaiveDate)> {
    match (
        args.start_date.as_deref(),
        args.end_date.as_deref(),
        args.lookback_trading_days,
    ) {
        (Some(start), Some(end), None) => Ok((parse_date(start)?, parse_date(end)?)),
        (None, None, Some(days)) => {
            let today = Utc::now()
                .with_timezone(&FixedOffset::east_opt(8 * 3600).expect("valid offset"))
                .date_naive();
            lookback_window(&cached_sessions(database, today)?, days)
        }
        _ => bail!("provide both --start-date and --end-date, or --lookback-trading-days"),
    }
}

pub(crate) fn run(args: Args) -> Result<()> {
    let database = args
        .database
        .clone()
        .or_else(|| std::env::var("DUCKDB_PATH").ok())
        .unwrap_or_else(|| "runtime/stock-cache.duckdb".into());
    ensure!(
        PathBuf::from(&database).is_file(),
        "database does not exist: {database}"
    );
    let (start, end) = resolve_window(&database, &args)?;
    ensure!(start <= end, "--start-date must be <= --end-date");
    let counts = duckdb::with_connection(&database, |connection| {
        duckdb::create_schema(connection)?;
        let mut targets = DATASETS
            .iter()
            .filter(|d| d.field("trade_date").is_some())
            .map(|d| (d.table, "trade_date"))
            .collect::<Vec<_>>();
        targets.extend([
            ("calc_period_bars", "period_end_date"),
            ("calc_indicator_state", "period_end_date"),
            ("calc_daily_indicators", "trade_date"),
            ("calc_daily_czsc_structure", "trade_date"),
            ("calc_daily_rolling_factors", "trade_date"),
            ("calc_daily_left_peak", "trade_date"),
            (duckdb::INDUSTRY_MONEYFLOW_TABLE, "trade_date"),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn day(text: &str) -> NaiveDate {
        parse_date(text).unwrap()
    }

    #[test]
    fn lookback_selects_the_most_recent_sessions() {
        let sessions = [day("2026-11-09"), day("2026-11-10"), day("2026-11-11")];
        assert_eq!(
            lookback_window(&sessions, 1).unwrap(),
            (day("2026-11-11"), day("2026-11-11"))
        );
        assert_eq!(
            lookback_window(&sessions, 2).unwrap(),
            (day("2026-11-10"), day("2026-11-11"))
        );
        assert_eq!(
            lookback_window(&sessions, 3).unwrap(),
            (day("2026-11-09"), day("2026-11-11"))
        );
        assert!(lookback_window(&sessions, 4).is_err());
        assert!(lookback_window(&[], 1).is_err());
    }
}
