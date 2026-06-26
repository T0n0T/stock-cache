use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use serde::Serialize;

use crate::{
    cache, calc,
    cli::{WriteArgs, WriteMode},
    config::Config,
};

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
    date_window: String,
    source_row_counts: BTreeMap<String, usize>,
    calc_source_row_count: usize,
    target_trade_dates: Vec<String>,
    calc_row_counts: BTreeMap<String, usize>,
    usable_ranges: BTreeMap<String, Vec<Option<String>>>,
    message: &'static str,
}

pub async fn run(args: WriteArgs, config: &Config) -> Result<()> {
    let window = resolve_date_window(&args, config)?;
    let _token_is_configured = !config.tushare_token.is_empty();
    if args.mode == WriteMode::Cache {
        let summary = cache::run_cache(&window, args.refresh_ths, config).await?;
        println!("{}", cache::summary_json(&summary)?);
        return Ok(());
    }
    if args.mode == WriteMode::Calc {
        let summary = calc::run_calc(&window, config).await?;
        println!("{}", calc::summary_json(&summary)?);
        return Ok(());
    }
    let cache_summary = cache::run_cache(&window, args.refresh_ths, config).await?;
    let calc_summary = calc::run_calc(&window, config).await?;
    let summary = full_summary(&window, cache_summary, calc_summary, args.refresh_ths);
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
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
    cache_summary: cache::CacheSummary,
    calc_summary: calc::CalcSummary,
    refresh_ths: bool,
) -> FullSummary {
    FullSummary {
        status: "success",
        mode: "full",
        refresh_ths,
        date_window: format_window(window),
        source_row_counts: cache_summary.source_row_counts,
        calc_source_row_count: calc_summary.source_row_count,
        target_trade_dates: calc_summary.target_trade_dates,
        calc_row_counts: calc_summary.calc_row_counts,
        usable_ranges: calc_summary.usable_ranges,
        message: "source cache write and local calculation completed",
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

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config {
            postgres_dsn: "postgres://example".to_string(),
            tushare_token: "token".to_string(),
            postgres_dsn_set: true,
            tushare_token_set: true,
            index_list_path: "runtime/default-indexes.csv".to_string(),
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
    fn full_summary_combines_cache_and_calc_counts() {
        let window = DateWindow::Explicit {
            start_date: "20250625".to_string(),
            end_date: "20250625".to_string(),
        };
        let cache_summary = cache::CacheSummary {
            status: "success",
            mode: "cache",
            refresh_ths: true,
            source_row_counts: BTreeMap::from([("stock_stk_factor_pro".to_string(), 5648)]),
            message: "source cache write completed",
        };
        let calc_summary = calc::CalcSummary {
            status: "success",
            mode: "calc",
            source_row_count: 5648,
            target_trade_dates: vec!["2025-06-25".to_string()],
            calc_row_counts: BTreeMap::from([("stock_daily_asof_indicators".to_string(), 5648)]),
            usable_ranges: BTreeMap::from([(
                "stock_daily_asof_indicators".to_string(),
                vec![
                    Some("2025-06-25".to_string()),
                    Some("2025-06-25".to_string()),
                ],
            )]),
            message: "local calculation completed",
        };

        let summary = full_summary(&window, cache_summary, calc_summary, true);

        assert_eq!(summary.status, "success");
        assert_eq!(summary.mode, "full");
        assert_eq!(summary.refresh_ths, true);
        assert_eq!(summary.date_window, "20250625..20250625");
        assert_eq!(
            summary.source_row_counts.get("stock_stk_factor_pro"),
            Some(&5648)
        );
        assert_eq!(
            summary.calc_row_counts.get("stock_daily_asof_indicators"),
            Some(&5648)
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
