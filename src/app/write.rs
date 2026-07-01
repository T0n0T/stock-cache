use std::{collections::BTreeMap, time::Instant};

use anyhow::{anyhow, Result};
use chrono::{Duration as ChronoDuration, Local, Months, NaiveDate};
use serde::Serialize;

use crate::{
    app::{config::Config, database_state},
    calc,
    cli::{WriteArgs, WriteMode},
    source::{cache, writer::parse_date_text},
};

const MONTHLY_MACD_WARMUP_MONTHS: u32 = 36;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum DateWindow {
    Explicit {
        start_date: String,
        end_date: String,
    },
    LookbackTradingDays(u32),
    DefaultLookbackTradingDays(u32),
    RefreshOnly,
}

#[derive(Debug, Serialize)]
struct FullSummary {
    status: &'static str,
    mode: &'static str,
    refresh: bool,
    calc_date_window: String,
    cache_date_window: String,
    calc_source_warmup_date_window: Option<String>,
    source_row_counts: BTreeMap<String, usize>,
    warmup_source_row_counts: BTreeMap<String, usize>,
    calc_source_row_count: usize,
    target_trade_dates: Vec<String>,
    calc_row_counts: BTreeMap<String, usize>,
    usable_ranges: BTreeMap<String, Vec<Option<String>>>,
    message: &'static str,
}

#[derive(Debug)]
struct FullTradeDatePlan {
    target_trade_dates: Vec<NaiveDate>,
    warmup_trade_dates: Vec<NaiveDate>,
}

pub async fn run(args: WriteArgs, config: &Config) -> Result<()> {
    let window = resolve_date_window(&args, config)?;
    let _token_is_configured = !config.tushare_token.is_empty();
    let max_concurrency = args.max_concurrency.unwrap_or(config.max_concurrency);
    let calc_chunk_size = args.chunk_size.or(config.calc_chunk_size);
    if args.mode == WriteMode::Cache {
        let summary = cache::run_cache(&window, args.refresh, config).await?;
        let summary_json = cache::summary_json(&summary)?;
        database_state::write_json(&summary_json)?;
        println!("{summary_json}");
        return Ok(());
    }
    if args.mode == WriteMode::Calc {
        let summary = calc::run_calc(
            &window,
            config,
            max_concurrency,
            args.force_recalc,
            calc_chunk_size,
        )
        .await?;
        let summary_json = calc::summary_json(&summary)?;
        database_state::write_json(&summary_json)?;
        println!("{summary_json}");
        return Ok(());
    }
    let full_started = Instant::now();
    let refresh_cache_summary = if args.refresh {
        eprintln!("[full] refresh stage start");
        let started = Instant::now();
        let summary = cache::run_cache(&DateWindow::RefreshOnly, true, config).await?;
        eprintln!(
            "[full] refresh stage done elapsed={:.1}s",
            started.elapsed().as_secs_f64()
        );
        Some(summary)
    } else {
        None
    };
    eprintln!("[full] trade calendar planning start");
    let started = Instant::now();
    let trade_date_plan = full_trade_date_plan(&window, config).await?;
    let calc_window = target_window_from_dates(&trade_date_plan.target_trade_dates)
        .unwrap_or_else(|| window.clone());
    eprintln!(
        "[full] trade calendar planning done target_days={} warmup_days={} elapsed={:.1}s",
        trade_date_plan.target_trade_dates.len(),
        trade_date_plan.warmup_trade_dates.len(),
        started.elapsed().as_secs_f64()
    );
    eprintln!("[full] target cache stage start");
    let started = Instant::now();
    let target_cache_summary =
        cache::run_cache_for_trade_dates(&trade_date_plan.target_trade_dates, false, config)
            .await?;
    eprintln!(
        "[full] target cache stage done elapsed={:.1}s",
        started.elapsed().as_secs_f64()
    );
    let warmup_cache_summary = if trade_date_plan.warmup_trade_dates.is_empty() {
        None
    } else {
        eprintln!("[full] calc source warmup stage start");
        let started = Instant::now();
        let summary = cache::run_calc_source_cache_for_trade_dates(
            &trade_date_plan.warmup_trade_dates,
            config,
        )
        .await?;
        eprintln!(
            "[full] calc source warmup stage done elapsed={:.1}s",
            started.elapsed().as_secs_f64()
        );
        Some(summary)
    };
    let warmup_window = warmup_window_from_dates(&trade_date_plan.warmup_trade_dates);
    eprintln!("[full] calc stage start");
    let started = Instant::now();
    let calc_summary = calc::run_calc(
        &calc_window,
        config,
        max_concurrency,
        args.force_recalc,
        calc_chunk_size,
    )
    .await?;
    eprintln!(
        "[full] calc stage done elapsed={:.1}s",
        started.elapsed().as_secs_f64()
    );
    let summary = full_summary(
        &calc_window,
        warmup_window.as_ref(),
        refresh_cache_summary,
        target_cache_summary,
        warmup_cache_summary,
        calc_summary,
        args.refresh,
    );
    eprintln!(
        "[full] done elapsed={:.1}s",
        full_started.elapsed().as_secs_f64()
    );
    let summary_json = serde_json::to_string_pretty(&summary)?;
    database_state::write_json(&summary_json)?;
    println!("{summary_json}");
    Ok(())
}

async fn full_trade_date_plan(window: &DateWindow, config: &Config) -> Result<FullTradeDatePlan> {
    match window {
        DateWindow::Explicit {
            start_date,
            end_date,
        } => {
            let start = parse_date_text(start_date)?;
            let end = parse_date_text(end_date)?;
            let query_start = start
                .checked_sub_months(Months::new(MONTHLY_MACD_WARMUP_MONTHS))
                .ok_or_else(|| anyhow!("failed to calculate monthly MACD warmup start date"))?;
            let open_dates = cache::trade_cal_open_dates(config, query_start, end).await?;
            full_trade_date_plan_from_open_dates(start, end, &open_dates)
        }
        DateWindow::LookbackTradingDays(days) | DateWindow::DefaultLookbackTradingDays(days) => {
            let as_of = lookback_trade_cal_as_of(Local::now().date_naive())?;
            let query_start = lookback_trade_cal_query_start(as_of, *days)?;
            let open_dates = cache::trade_cal_open_dates(config, query_start, as_of).await?;
            full_trade_date_plan_from_lookback_open_dates(as_of, *days, &open_dates)
        }
        DateWindow::RefreshOnly => Err(anyhow!(
            "full execution requires a stock_stk_factor_pro date window"
        )),
    }
}

fn full_trade_date_plan_from_open_dates(
    start: NaiveDate,
    end: NaiveDate,
    open_dates: &[NaiveDate],
) -> Result<FullTradeDatePlan> {
    let target_trade_dates = open_dates
        .iter()
        .copied()
        .filter(|date| *date >= start && *date <= end)
        .collect::<Vec<_>>();
    let Some(first_target_date) = target_trade_dates.first().copied() else {
        return Ok(FullTradeDatePlan {
            target_trade_dates,
            warmup_trade_dates: Vec::new(),
        });
    };
    let warmup_start = first_target_date
        .checked_sub_months(Months::new(MONTHLY_MACD_WARMUP_MONTHS))
        .ok_or_else(|| anyhow!("failed to calculate monthly MACD warmup start date"))?;
    let warmup_trade_dates = open_dates
        .iter()
        .copied()
        .filter(|date| *date >= warmup_start && *date < first_target_date)
        .collect::<Vec<_>>();
    if warmup_trade_dates.is_empty() {
        return Err(anyhow!(
            "trade_cal returned no open days in monthly MACD warmup range {}..{}",
            warmup_start.format("%Y%m%d"),
            first_target_date.format("%Y%m%d")
        ));
    }
    Ok(FullTradeDatePlan {
        target_trade_dates,
        warmup_trade_dates,
    })
}

fn full_trade_date_plan_from_lookback_open_dates(
    as_of: NaiveDate,
    days: u32,
    open_dates: &[NaiveDate],
) -> Result<FullTradeDatePlan> {
    let target_trade_dates = open_dates
        .iter()
        .copied()
        .filter(|date| *date <= as_of)
        .rev()
        .take(days as usize)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>();
    if target_trade_dates.len() < days as usize {
        return Err(anyhow!(
            "trade_cal returned only {} open days for --lookback-trading-days {} as of {}",
            target_trade_dates.len(),
            days,
            yyyymmdd(as_of)
        ));
    }
    let start = *target_trade_dates
        .first()
        .expect("checked enough target trade dates");
    let end = *target_trade_dates
        .last()
        .expect("checked enough target trade dates");
    full_trade_date_plan_from_open_dates(start, end, open_dates)
}

fn lookback_trade_cal_as_of(today: NaiveDate) -> Result<NaiveDate> {
    Ok(today)
}

fn lookback_trade_cal_query_start(as_of: NaiveDate, days: u32) -> Result<NaiveDate> {
    let target_horizon_days = i64::from(days).saturating_mul(3).saturating_add(30).max(90);
    let target_horizon = ChronoDuration::try_days(target_horizon_days)
        .ok_or_else(|| anyhow!("failed to calculate lookback trade calendar horizon"))?;
    let target_horizon_start = as_of
        .checked_sub_signed(target_horizon)
        .ok_or_else(|| anyhow!("failed to calculate lookback trade calendar start date"))?;
    target_horizon_start
        .checked_sub_months(Months::new(MONTHLY_MACD_WARMUP_MONTHS))
        .ok_or_else(|| anyhow!("failed to calculate monthly MACD warmup start date"))
}

pub fn resolve_date_window(args: &WriteArgs, config: &Config) -> Result<DateWindow> {
    let has_start = args.start_date.is_some();
    let has_end = args.end_date.is_some();
    if has_start != has_end {
        return Err(anyhow!(
            "--start-date and --end-date must be provided together"
        ));
    }
    if has_start && args.lookback_trading_days.is_some() {
        return Err(anyhow!(
            "--start-date/--end-date and --lookback-trading-days are mutually exclusive"
        ));
    }
    if let Some(max_concurrency) = args.max_concurrency {
        if max_concurrency == 0 {
            return Err(anyhow!("--max-concurrency must be greater than 0"));
        }
    }
    if let Some(chunk_size) = args.chunk_size {
        if chunk_size == 0 {
            return Err(anyhow!("--chunk-size must be greater than 0"));
        }
    }

    if args.refresh
        && args.mode == WriteMode::Cache
        && !has_start
        && args.lookback_trading_days.is_none()
    {
        return Ok(DateWindow::RefreshOnly);
    }
    if has_start {
        let start_date = args.start_date.clone().expect("checked has_start");
        let end_date = args.end_date.clone().expect("checked has_end");
        validate_yyyymmdd("--start-date", &start_date)?;
        validate_yyyymmdd("--end-date", &end_date)?;
        if start_date > end_date {
            return Err(anyhow!("--start-date must be <= --end-date"));
        }
        return Ok(DateWindow::Explicit {
            start_date,
            end_date,
        });
    }
    if let Some(days) = args.lookback_trading_days {
        if days == 0 {
            return Err(anyhow!("--lookback-trading-days must be greater than 0"));
        }
        return Ok(DateWindow::LookbackTradingDays(days));
    }
    Ok(DateWindow::DefaultLookbackTradingDays(
        config.default_lookback_trading_days,
    ))
}

fn validate_yyyymmdd(name: &str, value: &str) -> Result<()> {
    if value.len() != 8 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(anyhow!("{name} must use YYYYMMDD format"));
    }
    Ok(())
}

fn full_summary(
    window: &DateWindow,
    warmup_window: Option<&DateWindow>,
    refresh_cache_summary: Option<cache::CacheSummary>,
    target_cache_summary: cache::CacheSummary,
    warmup_cache_summary: Option<cache::CacheSummary>,
    calc_summary: calc::CalcSummary,
    refresh: bool,
) -> FullSummary {
    let warmup_source_row_counts = warmup_cache_summary
        .as_ref()
        .map(|summary| summary.source_row_counts.clone())
        .unwrap_or_default();
    let mut source_row_counts = refresh_cache_summary
        .map(|summary| summary.source_row_counts)
        .unwrap_or_default();
    merge_counts(
        &mut source_row_counts,
        &target_cache_summary.source_row_counts,
    );
    merge_counts(&mut source_row_counts, &warmup_source_row_counts);
    FullSummary {
        status: "success",
        mode: "full",
        refresh,
        calc_date_window: format_window(window),
        cache_date_window: format_window(window),
        calc_source_warmup_date_window: warmup_window.map(format_window),
        source_row_counts,
        warmup_source_row_counts,
        calc_source_row_count: calc_summary.source_row_count,
        target_trade_dates: calc_summary.target_trade_dates,
        calc_row_counts: calc_summary.calc_row_counts,
        usable_ranges: calc_summary.usable_ranges,
        message: "source cache write and local calculation completed",
    }
}

fn merge_counts(target: &mut BTreeMap<String, usize>, source: &BTreeMap<String, usize>) {
    for (table, count) in source {
        *target.entry(table.clone()).or_insert(0) += count;
    }
}

fn format_window(window: &DateWindow) -> String {
    match window {
        DateWindow::Explicit {
            start_date,
            end_date,
        } => format!("{start_date}..{end_date}"),
        DateWindow::LookbackTradingDays(days) => format!("lookback_trading_days:{days}"),
        DateWindow::DefaultLookbackTradingDays(days) => {
            format!("default_lookback_trading_days:{days}")
        }
        DateWindow::RefreshOnly => "refresh_only".to_string(),
    }
}

fn yyyymmdd(date: NaiveDate) -> String {
    date.format("%Y%m%d").to_string()
}

fn warmup_window_from_dates(dates: &[NaiveDate]) -> Option<DateWindow> {
    Some(DateWindow::Explicit {
        start_date: yyyymmdd(*dates.first()?),
        end_date: yyyymmdd(*dates.last()?),
    })
}

fn target_window_from_dates(dates: &[NaiveDate]) -> Option<DateWindow> {
    Some(DateWindow::Explicit {
        start_date: yyyymmdd(*dates.first()?),
        end_date: yyyymmdd(*dates.last()?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config {
            postgres_dsn: "postgres://example".to_string(),
            tushare_token: "token".to_string(),
            index_list_path: "config/default-indexes.csv".to_string(),
            default_lookback_trading_days: 90,
            max_concurrency: 4,
            cache_warmup_concurrency: 1,
            calc_chunk_size: None,
            request_timeout_seconds: 30,
            max_retries: 3,
            retry_base_delay_seconds: 1.0,
            write_batch_size: 1000,
            log_level: "info".to_string(),
        }
    }

    fn args(mode: WriteMode) -> WriteArgs {
        WriteArgs {
            mode,
            start_date: None,
            end_date: None,
            lookback_trading_days: None,
            max_concurrency: None,
            chunk_size: None,
            refresh: false,
            force_recalc: false,
        }
    }

    #[test]
    fn explicit_dates_must_be_paired() {
        let mut args = args(WriteMode::Full);
        args.start_date = Some("20260601".to_string());
        assert!(resolve_date_window(&args, &config()).is_err());
    }

    #[test]
    fn explicit_dates_and_lookback_are_mutually_exclusive() {
        let mut args = args(WriteMode::Full);
        args.start_date = Some("20260601".to_string());
        args.end_date = Some("20260625".to_string());
        args.lookback_trading_days = Some(30);
        assert!(resolve_date_window(&args, &config()).is_err());
    }

    #[test]
    fn cache_refresh_without_dates_is_refresh_only() {
        let mut args = args(WriteMode::Cache);
        args.refresh = true;
        assert_eq!(
            resolve_date_window(&args, &config()).unwrap(),
            DateWindow::RefreshOnly
        );
    }

    #[test]
    fn full_trade_date_plan_uses_three_calendar_year_monthly_macd_warmup() {
        let start = parse_date_text("20260601").unwrap();
        let end = parse_date_text("20260626").unwrap();
        let open_dates = [
            "20230531", "20230601", "20230602", "20260529", "20260601", "20260602", "20260626",
        ]
        .into_iter()
        .map(|date| parse_date_text(date).unwrap())
        .collect::<Vec<_>>();

        let plan = full_trade_date_plan_from_open_dates(start, end, &open_dates).unwrap();

        assert_eq!(
            plan.warmup_trade_dates,
            vec![
                parse_date_text("20230601").unwrap(),
                parse_date_text("20230602").unwrap(),
                parse_date_text("20260529").unwrap(),
            ]
        );
        assert_eq!(
            plan.target_trade_dates,
            vec![
                parse_date_text("20260601").unwrap(),
                parse_date_text("20260602").unwrap(),
                parse_date_text("20260626").unwrap(),
            ]
        );
    }

    #[test]
    fn full_trade_date_plan_handles_leap_day_three_calendar_year_warmup() {
        let start = parse_date_text("20240229").unwrap();
        let end = parse_date_text("20240301").unwrap();
        let open_dates = ["20210228", "20210301", "20240228", "20240229", "20240301"]
            .into_iter()
            .map(|date| parse_date_text(date).unwrap())
            .collect::<Vec<_>>();

        let plan = full_trade_date_plan_from_open_dates(start, end, &open_dates).unwrap();

        assert_eq!(
            plan.warmup_trade_dates,
            vec![
                parse_date_text("20210228").unwrap(),
                parse_date_text("20210301").unwrap(),
                parse_date_text("20240228").unwrap(),
            ]
        );
        assert_eq!(
            plan.target_trade_dates,
            vec![
                parse_date_text("20240229").unwrap(),
                parse_date_text("20240301").unwrap(),
            ]
        );
    }

    #[test]
    fn full_trade_date_plan_lookback_uses_latest_open_dates_as_targets() {
        let as_of = parse_date_text("20260629").unwrap();
        let open_dates = ["20230628", "20230629", "20260625", "20260626", "20260629"]
            .into_iter()
            .map(|date| parse_date_text(date).unwrap())
            .collect::<Vec<_>>();

        let plan = full_trade_date_plan_from_lookback_open_dates(as_of, 1, &open_dates).unwrap();

        assert_eq!(
            plan.warmup_trade_dates,
            vec![
                parse_date_text("20230629").unwrap(),
                parse_date_text("20260625").unwrap(),
                parse_date_text("20260626").unwrap(),
            ]
        );
        assert_eq!(
            plan.target_trade_dates,
            vec![parse_date_text("20260629").unwrap()]
        );
    }

    #[test]
    fn full_trade_date_plan_lookback_skips_future_open_dates() {
        let as_of = parse_date_text("20260628").unwrap();
        let open_dates = ["20230626", "20260625", "20260626", "20260629"]
            .into_iter()
            .map(|date| parse_date_text(date).unwrap())
            .collect::<Vec<_>>();

        let plan = full_trade_date_plan_from_lookback_open_dates(as_of, 1, &open_dates).unwrap();

        assert_eq!(
            plan.target_trade_dates,
            vec![parse_date_text("20260626").unwrap()]
        );
    }

    #[test]
    fn lookback_trade_cal_as_of_uses_current_calendar_day() {
        assert_eq!(
            lookback_trade_cal_as_of(parse_date_text("20260630").unwrap()).unwrap(),
            parse_date_text("20260630").unwrap()
        );
    }

    #[test]
    fn full_trade_date_plan_lookback_can_use_current_trading_day() {
        let as_of = lookback_trade_cal_as_of(parse_date_text("20260630").unwrap()).unwrap();
        let open_dates = ["20260626", "20260629", "20260630"]
            .into_iter()
            .map(|date| parse_date_text(date).unwrap())
            .collect::<Vec<_>>();

        let plan = full_trade_date_plan_from_lookback_open_dates(as_of, 1, &open_dates).unwrap();

        assert_eq!(
            plan.target_trade_dates,
            vec![parse_date_text("20260630").unwrap()]
        );
    }

    #[test]
    fn full_trade_date_plan_lookback_requires_enough_trade_dates() {
        let as_of = parse_date_text("20260629").unwrap();
        let open_dates = ["20260626"]
            .into_iter()
            .map(|date| parse_date_text(date).unwrap())
            .collect::<Vec<_>>();

        let error = full_trade_date_plan_from_lookback_open_dates(as_of, 2, &open_dates)
            .unwrap_err()
            .to_string();

        assert!(error.contains("trade_cal returned only 1 open days"));
    }

    #[test]
    fn target_window_uses_first_and_last_target_trade_dates() {
        let dates = ["20260626", "20260629"]
            .into_iter()
            .map(|date| parse_date_text(date).unwrap())
            .collect::<Vec<_>>();

        assert_eq!(
            target_window_from_dates(&dates),
            Some(DateWindow::Explicit {
                start_date: "20260626".to_string(),
                end_date: "20260629".to_string(),
            })
        );
    }

    #[test]
    fn full_summary_combines_cache_and_calc_counts() {
        let window = DateWindow::Explicit {
            start_date: "20250625".to_string(),
            end_date: "20250625".to_string(),
        };
        let target_cache_summary = cache::CacheSummary {
            status: "success",
            mode: "cache",
            refresh: true,
            source_row_counts: BTreeMap::from([("stock_stk_factor_pro".to_string(), 5648)]),
            message: "source cache write completed",
        };
        let warmup_window = DateWindow::Explicit {
            start_date: "20240102".to_string(),
            end_date: "20250624".to_string(),
        };
        let warmup_cache_summary = cache::CacheSummary {
            status: "success",
            mode: "cache",
            refresh: false,
            source_row_counts: BTreeMap::from([("stock_stk_factor_pro".to_string(), 1000)]),
            message: "calc source cache warmup completed",
        };
        let calc_summary = calc::CalcSummary {
            status: "success",
            mode: "calc",
            source_row_count: 6648,
            target_trade_dates: vec!["2025-06-25".to_string()],
            calc_row_counts: BTreeMap::from([
                ("stock_daily_asof_indicators".to_string(), 5648),
                ("index_daily_asof_indicators".to_string(), 100),
            ]),
            usable_ranges: BTreeMap::from([(
                "stock_daily_asof_indicators".to_string(),
                vec![
                    Some("2025-06-25".to_string()),
                    Some("2025-06-25".to_string()),
                ],
            )]),
            message: "local calculation completed",
        };

        let summary = full_summary(
            &window,
            Some(&warmup_window),
            None,
            target_cache_summary,
            Some(warmup_cache_summary),
            calc_summary,
            true,
        );

        assert_eq!(summary.status, "success");
        assert_eq!(summary.mode, "full");
        assert_eq!(summary.refresh, true);
        assert_eq!(summary.calc_date_window, "20250625..20250625");
        assert_eq!(summary.cache_date_window, "20250625..20250625");
        assert_eq!(
            summary.calc_source_warmup_date_window,
            Some("20240102..20250624".to_string())
        );
        assert_eq!(
            summary.source_row_counts.get("stock_stk_factor_pro"),
            Some(&6648)
        );
        assert_eq!(
            summary.warmup_source_row_counts.get("stock_stk_factor_pro"),
            Some(&1000)
        );
        assert_eq!(
            summary.calc_row_counts.get("stock_daily_asof_indicators"),
            Some(&5648)
        );
        assert_eq!(
            summary.calc_row_counts.get("index_daily_asof_indicators"),
            Some(&100)
        );
        assert_eq!(
            summary.usable_ranges.get("stock_daily_asof_indicators"),
            Some(&vec![
                Some("2025-06-25".to_string()),
                Some("2025-06-25".to_string())
            ])
        );
    }
}
