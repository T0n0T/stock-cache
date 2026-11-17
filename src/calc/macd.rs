use std::collections::HashMap;

use anyhow::{ensure, Result};
use chrono::{Datelike, Duration, NaiveDate};
use serde::Serialize;

const ALPHA_12: f64 = 2.0 / 13.0;
const ALPHA_26: f64 = 2.0 / 27.0;
const ALPHA_9: f64 = 2.0 / 10.0;

/// A source close for one instrument and one exchange trading date.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct DailyClose {
    pub trade_date: NaiveDate,
    pub close: Option<f64>,
}

/// MACD state after consuming one daily or completed-period close.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct MacdValue {
    pub ema12: f64,
    pub ema26: f64,
    pub dif: f64,
    pub dea: f64,
    pub hist_x2: f64,
    pub period_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum PeriodType {
    Weekly,
    Monthly,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct MacdAsofRow {
    pub trade_date: NaiveDate,
    pub daily: Option<MacdValue>,
    pub daily_dea_percentile: Option<f64>,
    pub weekly: Option<MacdValue>,
    pub weekly_dea_percentile: Option<f64>,
    pub monthly: Option<MacdValue>,
    pub monthly_dea_percentile: Option<f64>,
}

/// State recorded once for each completed week or month.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CompletedPeriodMacd {
    pub period_type: PeriodType,
    pub period_start_date: NaiveDate,
    /// Last exchange trading date in the calendar period, including holidays.
    pub period_end_date: NaiveDate,
    /// Last non-null source close observed in this instrument during the period.
    pub close: f64,
    pub state: MacdValue,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct MultiPeriodMacd {
    pub rows: Vec<MacdAsofRow>,
    pub completed_periods: Vec<CompletedPeriodMacd>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum PeriodKey {
    Weekly { year: i32, week: u32 },
    Monthly { year: i32, month: u32 },
}

#[derive(Debug, Clone, Copy)]
struct ActivePeriod {
    key: PeriodKey,
    start: NaiveDate,
    end: NaiveDate,
    close: Option<f64>,
}

impl ActivePeriod {
    fn new(period_type: PeriodType, date: NaiveDate, end: NaiveDate) -> Self {
        Self {
            key: period_key(period_type, date),
            start: period_start(period_type, date),
            end,
            close: None,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct MacdAccumulator {
    state: Option<MacdValue>,
}

impl MacdAccumulator {
    fn preview(&self, close: f64) -> MacdValue {
        match self.state {
            None => MacdValue {
                ema12: close,
                ema26: close,
                dif: 0.0,
                dea: 0.0,
                hist_x2: 0.0,
                period_count: 1,
            },
            Some(previous) => {
                let ema12 = ALPHA_12 * close + (1.0 - ALPHA_12) * previous.ema12;
                let ema26 = ALPHA_26 * close + (1.0 - ALPHA_26) * previous.ema26;
                let dif = ema12 - ema26;
                let dea = ALPHA_9 * dif + (1.0 - ALPHA_9) * previous.dea;
                MacdValue {
                    ema12,
                    ema26,
                    dif,
                    dea,
                    hist_x2: 2.0 * (dif - dea),
                    period_count: previous.period_count + 1,
                }
            }
        }
    }

    fn push(&mut self, close: f64) -> MacdValue {
        let state = self.preview(close);
        self.state = Some(state);
        state
    }
}

/// Calculate daily, weekly and monthly MACD as-of values for one instrument.
///
/// `trading_dates` must contain the ordered open sessions for the relevant exchange.
/// The calendar must cover each input bar's entire natural week and month, including later
/// sessions after the final bar when that period is not yet complete. Daily bars may omit
/// sessions (for example, a suspended instrument), but every supplied bar date must be
/// present in the calendar. A null close leaves recursive states unchanged and produces
/// null MACD values for that date.
pub fn calculate_multi_period_macd(
    bars: &[DailyClose],
    trading_dates: &[NaiveDate],
) -> Result<MultiPeriodMacd> {
    validate_strictly_increasing(trading_dates, "trading calendar")?;
    validate_strictly_increasing(
        &bars.iter().map(|bar| bar.trade_date).collect::<Vec<_>>(),
        "daily bars",
    )?;

    let mut week_ends = HashMap::new();
    let mut month_ends = HashMap::new();
    for &date in trading_dates {
        week_ends.insert(period_key(PeriodType::Weekly, date), date);
        month_ends.insert(period_key(PeriodType::Monthly, date), date);
    }

    let mut result = MultiPeriodMacd::default();
    let mut daily = MacdAccumulator::default();
    let mut weekly = MacdAccumulator::default();
    let mut monthly = MacdAccumulator::default();
    let mut daily_deas = Vec::new();
    let mut weekly_deas = Vec::new();
    let mut monthly_deas = Vec::new();
    let mut active_week: Option<ActivePeriod> = None;
    let mut active_month: Option<ActivePeriod> = None;

    for bar in bars {
        ensure!(
            trading_dates.binary_search(&bar.trade_date).is_ok(),
            "bar date {} is absent from trading calendar",
            bar.trade_date
        );
        if let Some(close) = bar.close {
            ensure!(close.is_finite(), "non-finite close on {}", bar.trade_date);
        }

        let week_key = period_key(PeriodType::Weekly, bar.trade_date);
        if active_week.is_some_and(|period| period.key != week_key) {
            commit_active(
                &mut active_week,
                &mut weekly,
                &mut weekly_deas,
                PeriodType::Weekly,
                &mut result.completed_periods,
            );
        }
        if active_week.is_none() {
            let end = *week_ends
                .get(&week_key)
                .ok_or_else(|| anyhow::anyhow!("no weekly calendar end for {}", bar.trade_date))?;
            active_week = Some(ActivePeriod::new(PeriodType::Weekly, bar.trade_date, end));
        }

        let month_key = period_key(PeriodType::Monthly, bar.trade_date);
        if active_month.is_some_and(|period| period.key != month_key) {
            commit_active(
                &mut active_month,
                &mut monthly,
                &mut monthly_deas,
                PeriodType::Monthly,
                &mut result.completed_periods,
            );
        }
        if active_month.is_none() {
            let end = *month_ends
                .get(&month_key)
                .ok_or_else(|| anyhow::anyhow!("no monthly calendar end for {}", bar.trade_date))?;
            active_month = Some(ActivePeriod::new(PeriodType::Monthly, bar.trade_date, end));
        }

        if let Some(close) = bar.close {
            active_week.as_mut().expect("initialized above").close = Some(close);
            active_month.as_mut().expect("initialized above").close = Some(close);
            let state = daily.push(close);
            daily_deas.push(state.dea);

            let weekly_state = weekly.preview(close);
            let monthly_state = monthly.preview(close);
            result.rows.push(MacdAsofRow {
                trade_date: bar.trade_date,
                daily: Some(state),
                daily_dea_percentile: Some(percentile_inclusive(&daily_deas, state.dea)),
                weekly: Some(weekly_state),
                weekly_dea_percentile: Some(percentile_with_current(
                    &weekly_deas,
                    weekly_state.dea,
                )),
                monthly: Some(monthly_state),
                monthly_dea_percentile: Some(percentile_with_current(
                    &monthly_deas,
                    monthly_state.dea,
                )),
            });
        } else {
            result.rows.push(MacdAsofRow {
                trade_date: bar.trade_date,
                daily: None,
                daily_dea_percentile: None,
                weekly: None,
                weekly_dea_percentile: None,
                monthly: None,
                monthly_dea_percentile: None,
            });
        }

        if active_week.is_some_and(|period| period.end == bar.trade_date) {
            commit_active(
                &mut active_week,
                &mut weekly,
                &mut weekly_deas,
                PeriodType::Weekly,
                &mut result.completed_periods,
            );
        }
        if active_month.is_some_and(|period| period.end == bar.trade_date) {
            commit_active(
                &mut active_month,
                &mut monthly,
                &mut monthly_deas,
                PeriodType::Monthly,
                &mut result.completed_periods,
            );
        }
    }

    Ok(result)
}

fn commit_active(
    active: &mut Option<ActivePeriod>,
    accumulator: &mut MacdAccumulator,
    deas: &mut Vec<f64>,
    period_type: PeriodType,
    output: &mut Vec<CompletedPeriodMacd>,
) {
    let Some(period) = active.take() else {
        return;
    };
    if let Some(close) = period.close {
        let state = accumulator.push(close);
        deas.push(state.dea);
        output.push(CompletedPeriodMacd {
            period_type,
            period_start_date: period.start,
            period_end_date: period.end,
            close,
            state,
        });
    }
}

fn validate_strictly_increasing(dates: &[NaiveDate], label: &str) -> Result<()> {
    for pair in dates.windows(2) {
        ensure!(pair[0] < pair[1], "{label} must be strictly increasing");
    }
    Ok(())
}

fn percentile_inclusive(values: &[f64], current: f64) -> f64 {
    values.iter().filter(|value| **value <= current).count() as f64 / values.len() as f64
}

fn percentile_with_current(previous: &[f64], current: f64) -> f64 {
    let below_or_equal = previous.iter().filter(|value| **value <= current).count() + 1;
    below_or_equal as f64 / (previous.len() + 1) as f64
}

fn period_key(period_type: PeriodType, date: NaiveDate) -> PeriodKey {
    match period_type {
        PeriodType::Weekly => {
            let iso = date.iso_week();
            PeriodKey::Weekly {
                year: iso.year(),
                week: iso.week(),
            }
        }
        PeriodType::Monthly => PeriodKey::Monthly {
            year: date.year(),
            month: date.month(),
        },
    }
}

fn period_start(period_type: PeriodType, date: NaiveDate) -> NaiveDate {
    match period_type {
        PeriodType::Weekly => {
            date - Duration::days(i64::from(date.weekday().num_days_from_monday()))
        }
        PeriodType::Monthly => {
            NaiveDate::from_ymd_opt(date.year(), date.month(), 1).expect("valid month start")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn weekdays(start: NaiveDate, end: NaiveDate) -> Vec<NaiveDate> {
        let mut dates = Vec::new();
        let mut current = start;
        while current <= end {
            if current.weekday().number_from_monday() <= 5 {
                dates.push(current);
            }
            current += Duration::days(1);
        }
        dates
    }

    #[test]
    fn daily_macd_matches_independent_pandas_ewm_golden_values() {
        // Generated with pandas Series.ewm(span=12/26/9, adjust=False), using the
        // first close as each EMA seed. This guards the recurrence and histogram scale.
        let calendar = weekdays(date(2024, 1, 1), date(2024, 1, 12));
        let closes = [10.0, 11.0, 9.0, 12.0, 13.0, 8.0, 8.5, 14.0, 15.0, 13.0];
        let bars = calendar
            .iter()
            .copied()
            .zip(closes)
            .map(|(trade_date, close)| DailyClose {
                trade_date,
                close: Some(close),
            })
            .collect::<Vec<_>>();
        let result = calculate_multi_period_macd(&bars, &calendar).unwrap();
        let last = result.rows.last().unwrap().daily.unwrap();
        assert!((last.ema12 - 11.5612668931).abs() < 1e-9);
        assert!((last.ema26 - 10.8634847539).abs() < 1e-9);
        assert!((last.dif - 0.6977821392).abs() < 1e-9);
        assert!((last.dea - 0.3094158699).abs() < 1e-9);
        assert!((last.hist_x2 - 0.7767325387).abs() < 1e-9);
        assert_eq!(last.period_count, 10);
    }

    #[test]
    fn weekly_and_monthly_previews_do_not_consume_partial_periods() {
        let calendar = weekdays(date(2024, 12, 16), date(2025, 1, 10));
        let bars = calendar
            .iter()
            .enumerate()
            .map(|(i, &trade_date)| DailyClose {
                trade_date,
                close: Some(100.0 + i as f64),
            })
            .collect::<Vec<_>>();
        let result = calculate_multi_period_macd(&bars, &calendar).unwrap();

        let monday = result
            .rows
            .iter()
            .find(|row| row.trade_date == date(2024, 12, 23))
            .unwrap();
        let friday = result
            .rows
            .iter()
            .find(|row| row.trade_date == date(2024, 12, 27))
            .unwrap();
        assert_eq!(monday.weekly.unwrap().period_count, 2);
        assert_eq!(friday.weekly.unwrap().period_count, 2);
        assert_ne!(monday.weekly.unwrap().dif, friday.weekly.unwrap().dif);

        let committed_weeks = result
            .completed_periods
            .iter()
            .filter(|period| period.period_type == PeriodType::Weekly)
            .collect::<Vec<_>>();
        assert_eq!(committed_weeks.len(), 4);
        assert_eq!(committed_weeks[0].period_end_date, date(2024, 12, 20));
        assert_eq!(committed_weeks[1].period_start_date, date(2024, 12, 23));

        let december = result
            .completed_periods
            .iter()
            .find(|period| {
                period.period_type == PeriodType::Monthly
                    && period.period_start_date == date(2024, 12, 1)
            })
            .unwrap();
        assert_eq!(december.period_end_date, date(2024, 12, 31));
    }

    #[test]
    fn exchange_calendar_end_closes_early_week_without_waiting_for_next_week_quote() {
        // Friday is a holiday; Thursday is the week's final exchange session.
        let mut calendar = weekdays(date(2025, 1, 1), date(2025, 2, 7));
        calendar.retain(|trading_date| *trading_date != date(2025, 1, 24));
        let bar_dates = [
            date(2025, 1, 20),
            date(2025, 1, 21),
            date(2025, 1, 22),
            date(2025, 1, 23),
        ];
        let bars = bar_dates
            .iter()
            .enumerate()
            .map(|(i, &trade_date)| DailyClose {
                trade_date,
                close: Some(20.0 + i as f64),
            })
            .collect::<Vec<_>>();
        let result = calculate_multi_period_macd(&bars, &calendar).unwrap();
        let last = result.rows.last().unwrap();
        assert_eq!(last.trade_date, date(2025, 1, 23));
        assert_eq!(last.weekly.unwrap().period_count, 1);
        assert!(result.completed_periods.iter().any(|period| {
            period.period_type == PeriodType::Weekly && period.period_end_date == date(2025, 1, 23)
        }));
        assert!(!result.completed_periods.iter().any(|period| {
            period.period_type == PeriodType::Monthly
                && period.period_start_date == date(2025, 1, 1)
        }));
    }

    #[test]
    fn iso_week_crossing_new_year_and_leap_february_have_calendar_boundaries() {
        let calendar = weekdays(date(2024, 12, 30), date(2025, 3, 3));
        let bars = calendar
            .iter()
            .enumerate()
            .map(|(i, &trade_date)| DailyClose {
                trade_date,
                close: Some(80.0 + i as f64 / 10.0),
            })
            .collect::<Vec<_>>();
        let result = calculate_multi_period_macd(&bars, &calendar).unwrap();
        let first_week = result
            .completed_periods
            .iter()
            .find(|period| period.period_type == PeriodType::Weekly)
            .unwrap();
        assert_eq!(first_week.period_start_date, date(2024, 12, 30));
        assert_eq!(first_week.period_end_date, date(2025, 1, 3));

        let february = result.completed_periods.iter().find(|period| {
            period.period_type == PeriodType::Monthly
                && period.period_start_date == date(2025, 2, 1)
        });
        assert_eq!(february.unwrap().period_end_date, date(2025, 2, 28));

        let leap_calendar = weekdays(date(2024, 1, 1), date(2024, 3, 8));
        let leap_bars = leap_calendar
            .iter()
            .map(|&trade_date| DailyClose {
                trade_date,
                close: Some(90.0),
            })
            .collect::<Vec<_>>();
        let leap_result = calculate_multi_period_macd(&leap_bars, &leap_calendar).unwrap();
        let leap_february = leap_result
            .completed_periods
            .iter()
            .find(|period| {
                period.period_type == PeriodType::Monthly
                    && period.period_start_date == date(2024, 2, 1)
            })
            .unwrap();
        assert_eq!(leap_february.period_end_date, date(2024, 2, 29));
    }

    #[test]
    fn future_bars_cannot_change_past_asof_values() {
        let calendar = weekdays(date(2025, 2, 3), date(2025, 3, 7));
        let bars = calendar
            .iter()
            .enumerate()
            .map(|(i, &trade_date)| DailyClose {
                trade_date,
                close: Some(50.0 + (i as f64 * 0.37).sin() * 4.0 + i as f64 * 0.1),
            })
            .collect::<Vec<_>>();
        let full = calculate_multi_period_macd(&bars, &calendar).unwrap();
        let prefix = calculate_multi_period_macd(&bars[..12], &calendar).unwrap();
        assert_eq!(prefix.rows, full.rows[..12]);
    }

    #[test]
    fn rejects_unsorted_duplicate_nonfinite_and_off_calendar_inputs() {
        let d1 = date(2025, 2, 3);
        let d2 = date(2025, 2, 4);
        assert!(calculate_multi_period_macd(
            &[
                DailyClose {
                    trade_date: d2,
                    close: Some(1.0)
                },
                DailyClose {
                    trade_date: d1,
                    close: Some(2.0)
                }
            ],
            &[d1, d2]
        )
        .is_err());
        assert!(calculate_multi_period_macd(
            &[DailyClose {
                trade_date: d1,
                close: Some(f64::NAN)
            }],
            &[d1]
        )
        .is_err());
        assert!(calculate_multi_period_macd(
            &[DailyClose {
                trade_date: d2,
                close: Some(1.0)
            }],
            &[d1]
        )
        .is_err());
        assert!(calculate_multi_period_macd(&[], &[d1, d1]).is_err());
    }

    #[test]
    fn null_close_does_not_advance_any_macd_state() {
        let calendar = weekdays(date(2025, 3, 3), date(2025, 3, 7));
        let bars = vec![
            DailyClose {
                trade_date: calendar[0],
                close: Some(10.0),
            },
            DailyClose {
                trade_date: calendar[1],
                close: None,
            },
            DailyClose {
                trade_date: calendar[2],
                close: Some(12.0),
            },
        ];
        let result = calculate_multi_period_macd(&bars, &calendar).unwrap();
        assert!(result.rows[1].daily.is_none());
        assert_eq!(result.rows[2].daily.unwrap().period_count, 2);
        assert_eq!(result.rows[2].weekly.unwrap().period_count, 1);
        assert_eq!(result.completed_periods.len(), 0);
    }
}
