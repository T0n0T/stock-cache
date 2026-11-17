//! Market-cache application workflow. Target dates and calculation history are separate windows.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
    time::Instant,
};

use anyhow::{ensure, Context, Result};
use chrono::{Duration, FixedOffset, Months, NaiveDate, Utc};
use serde_json::{json, Value};
use stock_cache::{
    metadata::tushare_fields::dataset, storage::duckdb, TushareHttpClient, TushareHttpConfig,
};

use super::market_data::{
    configured_indexes, configured_tables, coverage, date, env_number, execute, literal, refresh,
    strings, targets, write_rows, Job, CALC_SOURCES,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Full,
    Cache,
    Calc,
}

#[derive(Debug)]
pub(crate) struct Options {
    pub(crate) mode: Mode,
    pub(crate) database: Option<PathBuf>,
    pub(crate) start_date: Option<String>,
    pub(crate) end_date: Option<String>,
    pub(crate) lookback_trading_days: Option<u32>,
    pub(crate) max_concurrency: Option<u32>,
    pub(crate) chunk_size: Option<usize>,
    pub(crate) refresh: bool,
    pub(crate) force_recalc: bool,
}

pub(crate) fn run(args: Options) -> Result<()> {
    let started = Instant::now();
    let today = Utc::now()
        .with_timezone(&FixedOffset::east_opt(8 * 3600).unwrap())
        .date_naive();
    let end = args
        .end_date
        .as_deref()
        .map(date)
        .transpose()?
        .unwrap_or(today);
    let start = args.start_date.as_deref().map(date).transpose()?;
    ensure!(
        start.is_none_or(|start| start <= end),
        "--start-date must be <= --end-date"
    );
    ensure!(end <= today, "target end date must not be in the future");
    let days = args
        .lookback_trading_days
        .unwrap_or(u32::try_from(env_number(
            "DEFAULT_LOOKBACK_TRADING_DAYS",
            90,
        )?)?);
    ensure!(days > 0, "lookback must be positive");
    let concurrency = args
        .max_concurrency
        .map(|v| v as usize)
        .unwrap_or(env_number("MAX_CONCURRENCY", 4)?);
    let chunk_size = args
        .chunk_size
        .unwrap_or(env_number("CALC_CHUNK_SIZE", 64)?);
    ensure!(
        concurrency > 0 && chunk_size > 0,
        "concurrency and chunk size must be positive"
    );
    ensure!(
        !(args.mode == Mode::Calc && args.refresh),
        "--refresh requires cache or full mode"
    );
    let database = args
        .database
        .clone()
        .or_else(|| std::env::var_os("DUCKDB_PATH").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("runtime/stock-cache.duckdb"));
    if let Some(parent) = database.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let work = tempfile::tempdir()?;
    let runtime = tokio::runtime::Runtime::new()?;
    let client = if args.mode != Mode::Calc {
        Some(TushareHttpClient::new(TushareHttpConfig::from_env()?)?)
    } else {
        None
    };
    // v2's cache --refresh without a date window only refreshes reference data.
    if args.mode == Mode::Cache
        && args.refresh
        && start.is_none()
        && args.lookback_trading_days.is_none()
    {
        duckdb::with_connection(&database.to_string_lossy(), duckdb::create_schema)?;
        let mut counts = BTreeMap::new();
        refresh(
            &runtime,
            client.as_ref().expect("cache mode has a client"),
            &database,
            work.path(),
            &mut counts,
        )?;
        duckdb::with_connection(&database.to_string_lossy(), duckdb::checkpoint)?;
        println!(
            "{}",
            json!({"status":"success", "mode":"cache", "refresh_only":true, "database":database, "source_row_counts":counts, "total_ms":started.elapsed().as_secs_f64()*1000.0})
        );
        return Ok(());
    }
    let query_start = start
        .unwrap_or(
            end.checked_sub_signed(Duration::days(i64::from(days) * 3 + 90))
                .context("lookback date overflow")?,
        )
        .checked_sub_months(Months::new(36))
        .context("history date overflow")?;
    // Include future sessions through the following month, so previews do not
    // turn into completed weekly/monthly values just because input ends today.
    let calendar_end = end + Duration::days(40);
    let mut fetched_rows = BTreeMap::<String, usize>::new();
    let calendar = duckdb::with_connection(&database.to_string_lossy(), |connection| {
        duckdb::create_schema(connection)?;
        if let Some(client) = &client {
            let rows = runtime.block_on(client.fetch_api(
                "trade_cal",
                &["cal_date", "is_open"],
                &BTreeMap::from([
                    ("exchange".into(), json!("SSE")),
                    (
                        "start_date".into(),
                        json!(query_start.format("%Y%m%d").to_string()),
                    ),
                    (
                        "end_date".into(),
                        json!(calendar_end.format("%Y%m%d").to_string()),
                    ),
                ]),
            ))?;
            let mut open = BTreeSet::new();
            for row in rows {
                if row.get("is_open").is_some_and(|v| v == "1" || v == 1) {
                    let text = row
                        .get("cal_date")
                        .and_then(Value::as_str)
                        .context("calendar row missing cal_date")?;
                    open.insert(NaiveDate::parse_from_str(text, "%Y%m%d").or_else(|_| date(text))?);
                }
            }
            ensure!(
                open.iter().any(|d| *d > end),
                "trade calendar must include future sessions to distinguish incomplete periods"
            );
            ensure!(
                open.iter().any(|d| *d < end),
                "trade calendar has no historical sessions"
            );
            let values = open
                .iter()
                .map(|d| format!("(DATE '{d}')"))
                .collect::<Vec<_>>()
                .join(",");
            execute(connection, "BEGIN TRANSACTION")?;
            execute(connection, format!("DELETE FROM cache_trade_calendar WHERE trade_date BETWEEN DATE '{query_start}' AND DATE '{calendar_end}'"))?;
            execute(
                connection,
                format!("INSERT OR IGNORE INTO cache_trade_calendar VALUES {values}"),
            )?;
            execute(connection, "COMMIT")?;
        }
        strings(connection, format!("SELECT trade_date::VARCHAR FROM cache_trade_calendar WHERE trade_date <= DATE '{calendar_end}' ORDER BY trade_date"))?
            .iter().map(|s| date(s)).collect::<Result<Vec<_>>>()
    })?;
    ensure!(
        calendar.iter().any(|d| *d > end),
        "cached trade calendar lacks future sessions; run write --mode full/cache first"
    );
    let target = targets(&calendar, start, end, days)?;
    let from = target[0];
    let to = *target.last().unwrap();
    let history_from = from
        .checked_sub_months(Months::new(36))
        .context("history date overflow")?;
    let history = calendar
        .iter()
        .copied()
        .filter(|d| *d >= history_from && *d < from)
        .collect::<Vec<_>>();
    ensure!(
        !history.is_empty(),
        "calendar has no calculation warmup history"
    );
    eprintln!(
        "[write] mode={:?} targets={}..{} ({} sessions), warmup={} sessions",
        args.mode,
        from,
        to,
        target.len(),
        history.len()
    );
    if let Some(client) = client {
        let enabled = configured_tables()?;
        let indexes = configured_indexes()?;
        let mut jobs = Vec::new();
        let coverage = duckdb::with_connection(&database.to_string_lossy(), |connection| {
            coverage(connection)
        })?;
        for day in target.iter().chain(history.iter()) {
            let warmup = *day < from;
            for &id in &enabled {
                if warmup && (args.mode == Mode::Cache || !CALC_SOURCES.contains(&id)) {
                    continue;
                }
                let codes = if id == "index_idx_factor_pro" {
                    indexes.clone()
                } else {
                    vec![String::new()]
                };
                for code in codes {
                    let job = Job {
                        id,
                        day: *day,
                        code,
                    };
                    // Always refresh targets; only historical completed requests are reused.
                    if !warmup || args.force_recalc || !coverage.contains(&job.key()) {
                        jobs.push(job);
                    }
                }
            }
        }
        let total_jobs = jobs.len();
        duckdb::with_connection(&database.to_string_lossy(), |connection| {
            for (batch, jobs) in jobs.chunks(concurrency).enumerate() {
                let mut tasks = tokio::task::JoinSet::new();
                let responses = runtime.block_on(async {
                    for job in jobs.iter().cloned() {
                        let client = client.clone();
                        tasks.spawn(async move {
                            let params = job.params();
                            let rows = client
                                .fetch_dataset(dataset(job.id).unwrap(), &params)
                                .await?;
                            Ok::<_, anyhow::Error>((job, rows))
                        });
                    }
                    let mut responses = Vec::new();
                    while let Some(result) = tasks.join_next().await {
                        responses.push(result??);
                    }
                    Ok::<_, anyhow::Error>(responses)
                })?;
                for (job, mut rows) in responses {
                    if job.id == "index_moneyflow_cnt_ths" {
                        rows.retain(|row| {
                            row.get("ts_code")
                                .and_then(Value::as_str)
                                .is_some_and(|code| !code.is_empty())
                        });
                    }
                    if job.id == "stock_stk_factor_pro" {
                        ensure!(!rows.is_empty(), "Tushare has no stock market data for {}; target data may not be published yet", job.day);
                    }
                    for row in &rows {
                        ensure!(
                            row.get("trade_date")
                                .and_then(Value::as_str)
                                .is_some_and(|d| d == job.day.to_string()
                                    || d == job.day.format("%Y%m%d").to_string()),
                            "{} returned a row outside requested date {}",
                            job.id,
                            job.day
                        );
                        if !job.code.is_empty() {
                            ensure!(
                                row.get("ts_code").and_then(Value::as_str) == Some(&job.code),
                                "index response code mismatch"
                            );
                        }
                    }
                    let path = work.path().join("source.jsonl");
                    write_rows(&path, &rows)?;
                    let scope = if job.code.is_empty() {
                        BTreeMap::new()
                    } else {
                        BTreeMap::from([("ts_code".into(), job.code.clone())])
                    };
                    let written = duckdb::write_jsonl(
                        connection,
                        job.id,
                        &path,
                        "replace-dates",
                        &[job.day.to_string()],
                        &scope,
                        &[],
                    )?;
                    execute(connection, format!("INSERT OR REPLACE INTO cache_fetch_coverage VALUES ({}, DATE '{}', {}, {})", literal(job.id), job.day, literal(&job.code), written))?;
                    *fetched_rows.entry(job.id.into()).or_default() += rows.len();
                }
                if batch % 10 == 0 {
                    eprintln!(
                        "[write] fetched {}/{} requests",
                        ((batch + 1) * concurrency).min(total_jobs),
                        total_jobs
                    );
                }
            }
            Ok(())
        })?;
        if args.refresh {
            refresh(&runtime, &client, &database, work.path(), &mut fetched_rows)?;
        }
    }
    let fetch_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut calc_rows = 0usize;
    if args.mode != Mode::Cache {
        // Require source-history coverage, not just a recent row, before deriving factors.
        duckdb::with_connection(&database.to_string_lossy(), |connection| {
            let present = coverage(connection)?;
            let indexes = configured_indexes()?;
            for id in CALC_SOURCES {
                let codes = if id == "index_idx_factor_pro" {
                    indexes.clone()
                } else {
                    vec![String::new()]
                };
                for code in codes {
                    ensure!(history.iter().chain(target.iter()).all(|day| present.contains(&Job { id, day: *day, code: code.clone() }.key())),
                        "{id}/{code} lacks historical source coverage; run write --mode full to fill it");
                }
            }
            Ok(())
        })?;
        let calendar_path = work.path().join("calendar.txt");
        fs::write(
            &calendar_path,
            calendar
                .iter()
                .map(|d| format!("{d}\n"))
                .collect::<String>(),
        )?;
        for id in CALC_SOURCES {
            let codes = duckdb::with_connection(&database.to_string_lossy(), |connection| {
                strings(connection,
                format!("SELECT DISTINCT ts_code FROM {id} WHERE trade_date BETWEEN DATE '{from}' AND DATE '{to}' ORDER BY ts_code"))
            })?;
            for codes in codes.chunks(chunk_size) {
                let input = work.path().join("bars.jsonl");
                let output = work.path().join("calculated.jsonl");
                duckdb::with_connection(&database.to_string_lossy(), |connection| {
                    let adjusted = id == "stock_stk_factor_pro";
                    let prices = ["open", "high", "low", "close"]
                        .iter()
                        .map(|col| {
                            if adjusted {
                                format!("{col}_qfq AS {col}")
                            } else {
                                (*col).to_string()
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    let amount = if dataset(id).unwrap().field("amount").is_some() {
                        "amount"
                    } else {
                        "NULL::DOUBLE AS amount"
                    };
                    let turnover = if dataset(id).unwrap().field("turnover_rate").is_some() {
                        "turnover_rate"
                    } else {
                        "NULL::DOUBLE AS turnover_rate"
                    };
                    let codes = codes
                        .iter()
                        .map(|c| literal(c))
                        .collect::<Vec<_>>()
                        .join(",");
                    // Keep the original cache seed as it grows; never slide EMA's seed forward.
                    execute(connection, format!("COPY (SELECT {} AS source_dataset, ts_code, trade_date, {prices}, vol, {amount}, {turnover} FROM {id} WHERE ts_code IN ({codes}) AND trade_date <= DATE '{to}' ORDER BY ts_code, trade_date) TO {} (FORMAT JSON)", literal(id), literal(&input.to_string_lossy())))
                })?;
                super::calculation::run_cached(
                    &input,
                    &calendar_path,
                    &database,
                    &output,
                    from,
                    to,
                    if id == "stock_stk_factor_pro" {
                        "qfq"
                    } else {
                        "bfq"
                    },
                )?;
                calc_rows += fs::read_to_string(&output)?.lines().count();
            }
        }
        ensure!(
            calc_rows > 0,
            "no calculation output for requested trading dates"
        );
    }
    duckdb::with_connection(&database.to_string_lossy(), duckdb::checkpoint)?;
    println!(
        "{}",
        json!({"status":"success", "mode":format!("{:?}",args.mode).to_lowercase(), "database":database, "target_trade_dates":target, "warmup_start":history_from, "source_row_counts":fetched_rows, "calculated_rows":calc_rows, "fetch_write_ms":fetch_ms, "calc_write_ms":started.elapsed().as_secs_f64()*1000.0-fetch_ms, "total_ms":started.elapsed().as_secs_f64()*1000.0})
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::market_data::{date, targets};

    #[test]
    fn lookback_selects_sessions_not_calendar_days() {
        let open =
            ["2026-11-13", "2026-11-16", "2026-11-17", "2026-11-18"].map(|d| date(d).unwrap());
        assert_eq!(
            targets(&open, None, date("2026-11-15").unwrap(), 1).unwrap(),
            vec![open[0]]
        );
        assert_eq!(
            targets(&open, None, open[2], 2).unwrap(),
            vec![open[1], open[2]]
        );
        assert!(targets(&open, None, open[0], 2).is_err());
        assert_eq!(
            targets(&open, Some(open[1]), open[2], 90).unwrap(),
            vec![open[1], open[2]]
        );
    }
}
