use std::{collections::BTreeMap, time::Instant};

use anyhow::{anyhow, Result};
use chrono::{Months, NaiveDate};
use serde::Serialize;

use crate::{
    app::config::Config,
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
    RefreshThsOnly,
}

#[derive(Debug, Serialize)]
struct FullSummary {
    status: &'static str,
    mode: &'static str,
    refresh_ths: bool,
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

struct FullTradeDatePlan {
    target_trade_dates: Vec<NaiveDate>,
    warmup_trade_dates: Vec<NaiveDate>,
}

pub async fn run(args: WriteArgs, config: &Config) -> Result<()> {
    let window = resolve_date_window(&args, config)?;
    let _token_is_configured = !config.tushare_token.is_empty();
    let max_concurrency = args.max_concurrency.unwrap_or(config.max_concurrency);
    if args.mode == WriteMode::Cache {
        let summary = cache::run_cache(&window, args.refresh_ths, config).await?;
        println!("{}", cache::summary_json(&summary)?);
        return Ok(());
    }
    if args.mode == WriteMode::Calc {
        let summary = calc::run_calc(&window, config, max_concurrency, args.force_recalc).await?;
        println!("{}", calc::summary_json(&summary)?);
        return Ok(());
    }
    let full_started = Instant::now();
    let refresh_cache_summary = if args.refresh_ths {
        eprintln!("[full] refresh-ths stage start");
        let started = Instant::now();
        let summary = cache::run_cache(&DateWindow::RefreshThsOnly, true, config).await?;
        eprintln!(
            "[full] refresh-ths stage done elapsed={:.1}s",
            started.elapsed().as_secs_f64()
        );
        Some(summary)
    } else {
        None
    };
    eprintln!("[full] trade calendar planning start");
    let started = Instant::now();
    let trade_date_plan = full_trade_date_plan(&window, config).await?;
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
    let calc_summary = calc::run_calc(&window, config, max_concurrency, args.force_recalc).await?;
    eprintln!(
        "[full] calc stage done elapsed={:.1}s",
        started.elapsed().as_secs_f64()
    );
    let summary = full_summary(
        &window,
        warmup_window.as_ref(),
        refresh_cache_summary,
        target_cache_summary,
        warmup_cache_summary,
        calc_summary,
        args.refresh_ths,
    );
    eprintln!(
        "[full] done elapsed={:.1}s",
        full_started.elapsed().as_secs_f64()
    );
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

async fn full_trade_date_plan(window: &DateWindow, config: &Config) -> Result<FullTradeDatePlan> {
    let DateWindow::Explicit {
        start_date,
        end_date,
    } = window
    else {
        return Err(anyhow!(
            "cache execution currently requires explicit --start-date/--end-date or --refresh-ths-only"
        ));
    };
    let start = parse_date_text(start_date)?;
    let end = parse_date_text(end_date)?;
    let query_start = start
        .checked_sub_months(Months::new(MONTHLY_MACD_WARMUP_MONTHS))
        .ok_or_else(|| anyhow!("failed to calculate monthly MACD warmup start date"))?;
    let open_dates = cache::trade_cal_open_dates(config, query_start, end).await?;
    full_trade_date_plan_from_open_dates(start, end, &open_dates)
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

    if args.refresh_ths
        && args.mode == WriteMode::Cache
        && !has_start
        && args.lookback_trading_days.is_none()
    {
        return Ok(DateWindow::RefreshThsOnly);
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
    refresh_ths: bool,
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
        refresh_ths,
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
        DateWindow::RefreshThsOnly => "refresh_ths_only".to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config {
            postgres_dsn: "postgres://example".to_string(),
            tushare_token: "token".to_string(),
            postgres_dsn_set: true,
            tushare_token_set: true,
            index_list_path: "config/default-indexes.csv".to_string(),
            default_lookback_trading_days: 90,
            max_concurrency: 4,
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
            refresh_ths: false,
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
    fn cache_refresh_ths_without_dates_is_refresh_only() {
        let mut args = args(WriteMode::Cache);
        args.refresh_ths = true;
        assert_eq!(
            resolve_date_window(&args, &config()).unwrap(),
            DateWindow::RefreshThsOnly
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
    fn full_summary_combines_cache_and_calc_counts() {
        let window = DateWindow::Explicit {
            start_date: "20250625".to_string(),
            end_date: "20250625".to_string(),
        };
        let target_cache_summary = cache::CacheSummary {
            status: "success",
            mode: "cache",
            refresh_ths: true,
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
            refresh_ths: false,
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
        assert_eq!(summary.refresh_ths, true);
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
