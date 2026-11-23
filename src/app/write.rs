//! Market-cache application workflow. Target dates and calculation history are separate windows.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    time::{Duration as StdDuration, Instant},
};

use anyhow::{bail, ensure, Context, Result};
use chrono::{Duration, FixedOffset, Months, NaiveDate, Utc};
use serde_json::{json, Value};
use stock_cache::{
    metadata::tushare_fields::dataset, storage::duckdb, TushareHttpClient, TushareHttpConfig,
};

use super::market_data::{
    configured_indexes, configured_tables, coverage, date, env_number, execute, group_days,
    literal, refresh, row_counts, strings, targets, write_rows, Job, CALC_SOURCES,
};

// Range responses are written in bounded date chunks so each writer
// transaction deletes and inserts a small, indexed date slice.
const WRITE_CHUNK_DAYS: usize = 30;

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
    pub(crate) skip_covered: bool,
}

// Targets are normally refreshed even when already covered, so provider
// corrections reach the cache. Resuming a backfill opts out of that and reuses
// every completed request, warmup and target alike.
fn should_fetch(warmup: bool, skip_covered: bool, force_recalc: bool, covered: bool) -> bool {
    force_recalc || (!warmup && !skip_covered) || !covered
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Full => "full",
        Mode::Cache => "cache",
        Mode::Calc => "calc",
    }
}

fn format_count(count: usize) -> String {
    let digits = count.to_string();
    let mut formatted = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            formatted.push(',');
        }
        formatted.push(digit);
    }
    formatted
}

fn format_duration(milliseconds: f64) -> String {
    if milliseconds < 1_000.0 {
        return format!("{milliseconds:.0} ms");
    }
    let seconds = milliseconds / 1_000.0;
    if seconds < 60.0 {
        return format!("{seconds:.2} s");
    }
    let minutes = (seconds / 60.0).floor() as u64;
    format!("{minutes}m {:05.2}s", seconds - minutes as f64 * 60.0)
}

fn format_target_window(target: &[NaiveDate]) -> String {
    match target {
        [] => "none".to_string(),
        [day] => day.to_string(),
        [first, rest @ .., last] => format!("{first} to {last} ({} trading days)", rest.len() + 2),
    }
}

struct Summary<'a> {
    mode: Mode,
    database: &'a Path,
    target: Option<&'a [NaiveDate]>,
    warmup_start: Option<NaiveDate>,
    source_row_counts: &'a BTreeMap<String, usize>,
    calculated_rows: Option<usize>,
    fetch_write_ms: Option<f64>,
    calc_write_ms: Option<f64>,
    total_ms: f64,
    refresh_only: bool,
}

fn print_summary(summary: Summary<'_>) {
    let Summary {
        mode,
        database,
        target,
        warmup_start,
        source_row_counts,
        calculated_rows,
        fetch_write_ms,
        calc_write_ms,
        total_ms,
        refresh_only,
    } = summary;
    println!("Write completed successfully.");
    if refresh_only {
        println!("  Mode: {} (reference refresh only)", mode_name(mode));
    } else {
        println!("  Mode: {}", mode_name(mode));
    }
    println!("  Database: {}", database.display());
    if let Some(target) = target {
        println!("  Target dates: {}", format_target_window(target));
    }
    if let Some(warmup_start) = warmup_start {
        println!("  Warmup start: {warmup_start}");
    }

    if source_row_counts.is_empty() {
        println!("  Source rows fetched: none");
    } else {
        println!("  Source rows fetched:");
        for (source, count) in source_row_counts {
            println!("    {source}: {}", format_count(*count));
        }
    }
    if let Some(calculated_rows) = calculated_rows {
        println!("  Calculated rows: {}", format_count(calculated_rows));
    }

    println!("  Timing:");
    if let Some(fetch_write_ms) = fetch_write_ms {
        let label = if mode == Mode::Calc {
            "Prepare"
        } else {
            "Fetch/write"
        };
        println!("    {label}: {}", format_duration(fetch_write_ms));
    }
    if let Some(calc_write_ms) = calc_write_ms {
        println!("    Calculate/write: {}", format_duration(calc_write_ms));
    }
    println!("    Total: {}", format_duration(total_ms));
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
        print_summary(Summary {
            mode: Mode::Cache,
            database: &database,
            target: None,
            warmup_start: None,
            source_row_counts: &counts,
            calculated_rows: None,
            fetch_write_ms: None,
            calc_write_ms: None,
            total_ms: started.elapsed().as_secs_f64() * 1000.0,
            refresh_only: true,
        });
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
        let mut skipped = 0usize;
        let coverage = duckdb::with_connection(&database.to_string_lossy(), |connection| {
            coverage(connection)
        })?;
        let request_days = target
            .iter()
            .map(|day| (*day, false))
            .chain(history.iter().map(|day| (*day, true)))
            .collect::<Vec<_>>();
        for &id in &enabled {
            let indexed = id == "index_idx_factor_pro";
            let codes = if indexed {
                indexes.clone()
            } else {
                vec![String::new()]
            };
            for code in codes {
                let mut pending = Vec::new();
                for &(day, warmup) in &request_days {
                    if warmup && (args.mode == Mode::Cache || !CALC_SOURCES.contains(&id)) {
                        continue;
                    }
                    let key = format!("{id}|{day}|{code}");
                    if should_fetch(
                        warmup,
                        args.skip_covered,
                        args.force_recalc,
                        coverage.contains(&key),
                    ) {
                        if indexed {
                            pending.push(day);
                        } else {
                            jobs.push(Job::single(id, day, code.clone()));
                        }
                    } else {
                        skipped += 1;
                    }
                }
                // One provider request per index and contiguous date range;
                // writing still happens in bounded date chunks below.
                for days in group_days(pending) {
                    let day = days[0];
                    jobs.push(Job {
                        id,
                        day,
                        days,
                        code: code.clone(),
                    });
                }
            }
        }
        let total_jobs = jobs.len();
        if args.skip_covered {
            eprintln!("[write] resume: {skipped} covered requests skipped, {total_jobs} to fetch");
        }
        let request_window = client.config().requests_per_minute;
        eprintln!(
            "[write] request budget: at most {request_window} jobs per 60s; transport also counts pagination and retries"
        );
        duckdb::with_connection(&database.to_string_lossy(), |connection| {
            let mut window_started: Option<Instant> = None;
            for (window, window_jobs) in jobs.chunks(request_window).enumerate() {
                if let Some(started) = window_started {
                    let elapsed = started.elapsed();
                    let window_duration = StdDuration::from_secs(60);
                    if elapsed < window_duration {
                        eprintln!(
                            "[write] request budget reached; waiting {:.1}s before next batch",
                            (window_duration - elapsed).as_secs_f64()
                        );
                        runtime.block_on(async {
                            tokio::time::sleep(window_duration - elapsed).await;
                        });
                    }
                }
                window_started = Some(Instant::now());
                for (batch, jobs) in window_jobs.chunks(concurrency).enumerate() {
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
                    for (job, rows) in responses {
                        let allowed = job.days.iter().copied().collect::<BTreeSet<_>>();
                        let mut by_day = BTreeMap::<NaiveDate, Vec<BTreeMap<String, Value>>>::new();
                        for row in rows {
                            let text = row
                                .get("trade_date")
                                .and_then(Value::as_str)
                                .context("Tushare row missing trade_date")?;
                            let row_day = NaiveDate::parse_from_str(text, "%Y%m%d")
                                .or_else(|_| date(text))?;
                            if !allowed.contains(&row_day) {
                                // Range requests may include exchange-holiday rows.
                                // A single-day request must never return another date.
                                if job.days.len() == 1 {
                                    bail!(
                                        "{} returned a row outside requested date {}",
                                        job.id,
                                        job.day
                                    );
                                }
                                continue;
                            }
                            if !job.code.is_empty() {
                                ensure!(
                                    row.get("ts_code").and_then(Value::as_str) == Some(&job.code),
                                    "index response code mismatch"
                                );
                            }
                            by_day.entry(row_day).or_default().push(row);
                        }
                        if job.id == "stock_stk_factor_pro" {
                            ensure!(!by_day.is_empty(), "Tushare has no stock market data for {}; target data may not be published yet", job.day);
                        }
                        let scope = if job.code.is_empty() {
                            BTreeMap::new()
                        } else {
                            BTreeMap::from([("ts_code".into(), job.code.clone())])
                        };
                        // A range response can cover years of sessions; replace
                        // dates in bounded chunks so one transaction stays small.
                        for chunk in job.days.chunks(WRITE_CHUNK_DAYS) {
                            let mut chunk_rows = Vec::new();
                            for day in chunk {
                                if let Some(day_rows) = by_day.get(day) {
                                    chunk_rows.extend(day_rows.iter().cloned());
                                }
                            }
                            let path = work.path().join("source.jsonl");
                            write_rows(&path, &chunk_rows)?;
                            let dates = chunk.iter().map(|day| day.to_string()).collect::<Vec<_>>();
                            duckdb::write_jsonl(
                                connection,
                                job.id,
                                &path,
                                "replace-dates",
                                &dates,
                                &scope,
                                &[],
                            )?;
                            let counts = row_counts(connection, job.id, chunk, &job.code)?;
                            for day in chunk {
                                let count = counts.get(day).copied().unwrap_or(0);
                                execute(connection, format!("INSERT OR REPLACE INTO cache_fetch_coverage VALUES ({}, DATE '{}', {}, {})", literal(job.id), day, literal(&job.code), count))?;
                            }
                        }
                        *fetched_rows.entry(job.id.into()).or_default() +=
                            by_day.values().map(Vec::len).sum::<usize>();
                    }
                    let fetched = (window * request_window + batch * concurrency + jobs.len())
                        .min(total_jobs);
                    if batch % 10 == 0 || fetched == total_jobs {
                        eprintln!("[write] fetched {fetched}/{total_jobs} requests");
                    }
                }
            }
            Ok(())
        })?;
        if args.refresh {
            refresh(&runtime, &client, &database, work.path(), &mut fetched_rows)?;
        }
    }
    // The industry rollup is a cheap local derivation of the fetched stock
    // moneyflow.  Run it in cache mode too, so a moneyflow backfill does not
    // need to recompute every instrument indicator.
    let industry_rows = duckdb::with_connection(&database.to_string_lossy(), |connection| {
        super::industry_moneyflow::rebuild(connection, &target, duckdb::INDUSTRY_MONEYFLOW_VERSION)
    })?;
    if industry_rows > 0 {
        eprintln!("[write] industry moneyflow: wrote {industry_rows} rows");
    }
    let fetch_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut calc_rows = 0usize;
    if args.mode != Mode::Cache {
        eprintln!("[write] fetch complete; deriving industry moneyflow and calculating indicators");
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
                    ensure!(history.iter().chain(target.iter()).all(|day| present.contains(&Job::single(id, *day, code.clone()).key())),
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
            let total_codes = codes.len();
            eprintln!("[write] calculating {id}: 0/{total_codes} instruments");
            for (batch, codes) in codes.chunks(chunk_size).enumerate() {
                let input = work.path().join("bars.jsonl");
                let output = work.path().join("calculated.jsonl");
                duckdb::with_connection(&database.to_string_lossy(), |connection| {
                    let adjusted = id == "stock_stk_factor_pro";
                    let prices = ["open", "high", "low", "close"]
                        .iter()
                        .map(|col| {
                            if adjusted {
                                format!("source.{col}_qfq AS {col}")
                            } else {
                                format!("source.{col}")
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    let amount = if dataset(id).unwrap().field("amount").is_some() {
                        "source.amount"
                    } else {
                        "NULL::DOUBLE AS amount"
                    };
                    let turnover = if dataset(id).unwrap().field("turnover_rate").is_some() {
                        "source.turnover_rate"
                    } else {
                        "NULL::DOUBLE AS turnover_rate"
                    };
                    let codes = codes
                        .iter()
                        .map(|c| literal(c))
                        .collect::<Vec<_>>()
                        .join(",");
                    // Keep the original cache seed as it grows; never slide EMA's seed forward.
                    // Some index endpoints can return rows for exchange holidays.  The
                    // calculation calendar is authoritative, so exclude those rows instead
                    // of making the indicator calculator treat a holiday as a trading session.
                    execute(connection, format!("COPY (SELECT {} AS source_dataset, source.ts_code, source.trade_date, {prices}, source.vol, {amount}, {turnover} FROM {id} AS source INNER JOIN cache_trade_calendar AS calendar ON calendar.trade_date = source.trade_date WHERE source.ts_code IN ({codes}) AND source.trade_date <= DATE '{to}' ORDER BY source.ts_code, source.trade_date) TO {} (FORMAT JSON)", literal(id), literal(&input.to_string_lossy())))
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
                eprintln!(
                    "[write] calculating {id}: {}/{total_codes} instruments",
                    ((batch + 1) * chunk_size).min(total_codes)
                );
            }
        }
        ensure!(
            calc_rows > 0,
            "no calculation output for requested trading dates"
        );
    }
    duckdb::with_connection(&database.to_string_lossy(), duckdb::checkpoint)?;
    let total_ms = started.elapsed().as_secs_f64() * 1000.0;
    print_summary(Summary {
        mode: args.mode,
        database: &database,
        target: Some(&target),
        warmup_start: Some(history_from),
        source_row_counts: &fetched_rows,
        calculated_rows: (args.mode != Mode::Cache).then_some(calc_rows),
        fetch_write_ms: Some(fetch_ms),
        calc_write_ms: (args.mode != Mode::Cache).then_some(total_ms - fetch_ms),
        total_ms,
        refresh_only: false,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::market_data::{date, group_days, targets};
    use super::should_fetch;

    #[test]
    fn index_ranges_keep_holiday_gaps_and_cap_long_runs() {
        let split = group_days(vec![
            date("2026-11-09").unwrap(),
            date("2026-11-10").unwrap(),
            date("2026-11-21").unwrap(),
        ]);
        assert_eq!(split.len(), 2);
        assert_eq!(split[0].len(), 2);
        assert_eq!(split[1], vec![date("2026-11-21").unwrap()]);

        let start = date("2026-01-01").unwrap();
        let long = (0..260)
            .map(|offset| start + chrono::Duration::days(offset))
            .collect();
        let capped = group_days(long);
        assert_eq!(capped.len(), 2);
        assert_eq!(capped[0].len(), 250);
        assert_eq!(capped[1].len(), 10);
    }

    #[test]
    fn skip_covered_reuses_completed_requests() {
        // Ordinary runs refresh targets but reuse covered warmup history.
        assert!(should_fetch(false, false, false, true));
        assert!(!should_fetch(true, false, false, true));
        assert!(should_fetch(true, false, false, false));
        // Resuming skips targets and warmup alike once covered.
        assert!(!should_fetch(false, true, false, true));
        assert!(!should_fetch(true, true, false, true));
        assert!(should_fetch(false, true, false, false));
        assert!(should_fetch(true, true, false, false));
        // An explicit recalculation still refetches covered history.
        assert!(should_fetch(true, false, true, true));
    }

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
