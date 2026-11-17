//! Checkpoint metadata and invalidation planning for incremental v3 recomputes.

use anyhow::{ensure, Result};
use chrono::{Datelike, Duration, NaiveDate};

/// A persisted state row from `calc_indicator_state`.
#[derive(Debug, Clone, PartialEq)]
pub struct IndicatorCheckpoint {
    pub source_dataset: String,
    pub ts_code: String,
    pub period_type: String,
    pub period_start_date: NaiveDate,
    pub period_end_date: NaiveDate,
    pub macd_ema12: Option<f64>,
    pub macd_ema26: Option<f64>,
    pub macd_dea: Option<f64>,
    pub macd_period_count: Option<i64>,
    pub kdj_k: Option<f64>,
    pub kdj_d: Option<f64>,
    pub kdj_period_count: Option<i64>,
    pub calc_version: String,
}

/// Date boundaries used to select the rows that must be recomputed after a source change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecomputePlan {
    pub changed_from: NaiveDate,
    pub macd_from: NaiveDate,
    pub rolling_from: NaiveDate,
    pub left_peak_from: NaiveDate,
    pub czsc_from: NaiveDate,
    pub output_from: NaiveDate,
    pub daily_checkpoint_end: Option<NaiveDate>,
}

/// Build a conservative invalidation plan for one source instrument.
///
/// The daily/weekly/monthly MACD recursion may resume after the last completed
/// state before the changed date. Rolling windows and left-peak detection need
/// their historical warmup rows. CZSC is intentionally invalidated from the
/// changed date onward because its structure can depend on every prior bar.
pub fn plan_recompute(
    changed_from: NaiveDate,
    trading_dates: &[NaiveDate],
    checkpoints: &[IndicatorCheckpoint],
) -> Result<RecomputePlan> {
    ensure!(
        !trading_dates.is_empty(),
        "trading calendar must not be empty"
    );
    for pair in trading_dates.windows(2) {
        ensure!(
            pair[0] < pair[1],
            "trading calendar must be strictly increasing"
        );
    }
    let changed_index = trading_dates
        .binary_search(&changed_from)
        .unwrap_or_else(|index| index.min(trading_dates.len().saturating_sub(1)));
    let changed_session = trading_dates[changed_index];

    let daily_checkpoint_end = checkpoints
        .iter()
        .filter(|row| row.period_type == "daily" && row.period_end_date < changed_session)
        .map(|row| row.period_end_date)
        .max();
    let macd_from = daily_checkpoint_end
        .and_then(|date| next_date(trading_dates, date))
        .unwrap_or(trading_dates[0]);
    let rolling_from = offset_session(trading_dates, changed_index, 219);
    let left_peak_from = offset_session(trading_dates, changed_index, 360 + 2 * 2 + 5);
    let czsc_from = changed_session;
    let output_from = [macd_from, rolling_from, left_peak_from, czsc_from]
        .into_iter()
        .min()
        .unwrap_or(trading_dates[0]);

    Ok(RecomputePlan {
        changed_from,
        macd_from,
        rolling_from,
        left_peak_from,
        czsc_from,
        output_from,
        daily_checkpoint_end,
    })
}

fn next_date(dates: &[NaiveDate], date: NaiveDate) -> Option<NaiveDate> {
    let index = dates.binary_search(&date).ok()?;
    dates.get(index + 1).copied()
}

fn offset_session(dates: &[NaiveDate], index: usize, lookback: usize) -> NaiveDate {
    dates[index.saturating_sub(lookback)]
}

/// Natural period start used when selecting completed period rows for replacement.
pub fn period_start(date: NaiveDate, period_type: &str) -> NaiveDate {
    if period_type == "weekly" {
        date - Duration::days(i64::from(date.weekday().num_days_from_monday()))
    } else {
        NaiveDate::from_ymd_opt(date.year(), date.month(), 1).expect("valid month start")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dates() -> Vec<NaiveDate> {
        (1..=20)
            .map(|day| NaiveDate::from_ymd_opt(2025, 1, day).unwrap())
            .collect()
    }

    #[test]
    fn uses_previous_daily_checkpoint_and_window_warmups() {
        let calendar = dates();
        let checkpoints = vec![IndicatorCheckpoint {
            source_dataset: "demo".to_string(),
            ts_code: "000001.SZ".to_string(),
            period_type: "daily".to_string(),
            period_start_date: calendar[9],
            period_end_date: calendar[9],
            macd_ema12: Some(1.0),
            macd_ema26: Some(1.0),
            macd_dea: Some(0.0),
            macd_period_count: Some(10),
            kdj_k: None,
            kdj_d: None,
            kdj_period_count: None,
            calc_version: "v3".to_string(),
        }];
        let plan = plan_recompute(calendar[12], &calendar, &checkpoints).unwrap();
        assert_eq!(plan.daily_checkpoint_end, Some(calendar[9]));
        assert_eq!(plan.macd_from, calendar[10]);
        assert_eq!(plan.rolling_from, calendar[0]);
        assert_eq!(plan.output_from, calendar[0]);
    }

    #[test]
    fn missing_checkpoint_recomputes_from_calendar_start() {
        let calendar = dates();
        let plan = plan_recompute(calendar[5], &calendar, &[]).unwrap();
        assert_eq!(plan.macd_from, calendar[0]);
        assert_eq!(plan.left_peak_from, calendar[0]);
    }
}
