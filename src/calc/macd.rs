use chrono::{Datelike, Duration, NaiveDate};

use super::{
    constants::{ALPHA12, ALPHA26, ALPHA9},
    model::{AsofRow, PeriodStateRow, SourceBar},
};

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct MacdAccumulator {
    state: Option<MacdState>,
}

impl MacdAccumulator {
    pub(super) fn push(&mut self, close: f64) -> MacdState {
        let next = self.preview_next(close);
        self.state = Some(next);
        next
    }

    pub(super) fn preview_next(&self, close: f64) -> MacdState {
        match self.state {
            None => MacdState {
                ema12: close,
                ema26: close,
                dif: 0.0,
                dea: 0.0,
                hist_x2: 0.0,
                period_count: 1,
            },
            Some(previous) => {
                let ema12 = ALPHA12 * close + (1.0 - ALPHA12) * previous.ema12;
                let ema26 = ALPHA26 * close + (1.0 - ALPHA26) * previous.ema26;
                let dif = ema12 - ema26;
                let dea = ALPHA9 * dif + (1.0 - ALPHA9) * previous.dea;
                MacdState {
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
}

#[derive(Debug, Clone, Copy)]
pub(super) struct MacdState {
    pub(super) ema12: f64,
    pub(super) ema26: f64,
    pub(super) dif: f64,
    pub(super) dea: f64,
    pub(super) hist_x2: f64,
    pub(super) period_count: i32,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum PeriodKind {
    Weekly,
    Monthly,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum PeriodKey {
    Weekly { year: i32, week: u32 },
    Monthly { year: i32, month: u32 },
}

#[derive(Debug)]
pub(super) struct PeriodBuilder {
    kind: PeriodKind,
    key: PeriodKey,
    pub(super) start_date: NaiveDate,
    pub(super) end_date: NaiveDate,
    close_qfq: Option<f64>,
}

impl PeriodBuilder {
    pub(super) fn new(kind: PeriodKind, date: NaiveDate) -> Self {
        Self {
            kind,
            key: period_key(kind, date),
            start_date: period_start_date(kind, date),
            end_date: date,
            close_qfq: None,
        }
    }

    pub(super) fn contains(&self, date: NaiveDate) -> bool {
        self.key == period_key(self.kind, date)
    }

    pub(super) fn observe(&mut self, bar: &SourceBar) {
        self.end_date = bar.trade_date;
        if let Some(close) = bar.close_qfq {
            self.close_qfq = Some(close);
        }
    }
}

fn period_key(kind: PeriodKind, date: NaiveDate) -> PeriodKey {
    match kind {
        PeriodKind::Weekly => {
            let iso = date.iso_week();
            PeriodKey::Weekly {
                year: iso.year(),
                week: iso.week(),
            }
        }
        PeriodKind::Monthly => PeriodKey::Monthly {
            year: date.year(),
            month: date.month(),
        },
    }
}

pub(super) fn period_start_date(kind: PeriodKind, date: NaiveDate) -> NaiveDate {
    match kind {
        PeriodKind::Weekly => {
            date - Duration::days(i64::from(date.weekday().num_days_from_monday()))
        }
        PeriodKind::Monthly => {
            NaiveDate::from_ymd_opt(date.year(), date.month(), 1).expect("valid month start")
        }
    }
}

pub(super) fn finalize_period(
    ts_code: &str,
    period_type: &str,
    period: &PeriodBuilder,
    macd: &mut MacdAccumulator,
    deas: &mut Vec<f64>,
    rows: &mut Vec<PeriodStateRow>,
) {
    if let Some(close) = period.close_qfq {
        let state = macd.push(close);
        deas.push(state.dea);
        rows.push(PeriodStateRow::from_state(
            ts_code,
            period_type,
            period.start_date,
            period.end_date,
            close,
            state,
        ));
    }
}

pub(super) fn compute_asof_row(
    bar: &SourceBar,
    daily_state: Option<MacdState>,
    daily_deas: &[f64],
    weekly_macd: &MacdAccumulator,
    weekly_deas: &[f64],
    monthly_macd: &MacdAccumulator,
    monthly_deas: &[f64],
) -> AsofRow {
    let weekly_state = bar.close_qfq.map(|close| weekly_macd.preview_next(close));
    let monthly_state = bar.close_qfq.map(|close| monthly_macd.preview_next(close));
    AsofRow {
        ts_code: bar.ts_code.clone(),
        trade_date: bar.trade_date,
        daily_dif_asof: daily_state.map(|state| state.dif),
        daily_dea_asof: daily_state.map(|state| state.dea),
        daily_hist_x2_asof: daily_state.map(|state| state.hist_x2),
        daily_dea_pctile_asof: daily_state
            .and_then(|state| percentile_inclusive(daily_deas, state.dea)),
        daily_period_count: daily_state.map(|state| state.period_count),
        weekly_dif_asof: weekly_state.map(|state| state.dif),
        weekly_dea_asof: weekly_state.map(|state| state.dea),
        weekly_hist_x2_asof: weekly_state.map(|state| state.hist_x2),
        weekly_dea_pctile_asof: weekly_state
            .map(|state| percentile_with_current(weekly_deas, state.dea)),
        weekly_period_count: weekly_state.map(|state| state.period_count),
        monthly_dif_asof: monthly_state.map(|state| state.dif),
        monthly_dea_asof: monthly_state.map(|state| state.dea),
        monthly_hist_x2_asof: monthly_state.map(|state| state.hist_x2),
        monthly_dea_pctile_asof: monthly_state
            .map(|state| percentile_with_current(monthly_deas, state.dea)),
        monthly_period_count: monthly_state.map(|state| state.period_count),
    }
}

fn percentile_inclusive(values: &[f64], current: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    Some(values.iter().filter(|value| **value <= current).count() as f64 / values.len() as f64)
}

fn percentile_with_current(previous: &[f64], current: f64) -> f64 {
    let less_or_equal = previous.iter().filter(|value| **value <= current).count() + 1;
    less_or_equal as f64 / (previous.len() + 1) as f64
}

pub(super) fn close_or_nan(close: Option<f64>) -> f64 {
    close.expect("MACD state only exists with close_qfq")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macd_seeds_first_close_and_recurs() {
        let mut macd = MacdAccumulator::default();
        let first = macd.push(10.0);
        assert_eq!(first.ema12, 10.0);
        assert_eq!(first.ema26, 10.0);
        assert_eq!(first.dif, 0.0);
        assert_eq!(first.dea, 0.0);

        let second = macd.push(11.0);
        assert!(second.dif > 0.0);
        assert_eq!(second.period_count, 2);
        assert!((second.hist_x2 - 2.0 * (second.dif - second.dea)).abs() < 1e-12);
    }

    #[test]
    fn period_start_dates_use_natural_week_and_month_bounds() {
        let date = NaiveDate::from_ymd_opt(2025, 6, 25).unwrap();
        assert_eq!(
            period_start_date(PeriodKind::Weekly, date),
            NaiveDate::from_ymd_opt(2025, 6, 23).unwrap()
        );
        assert_eq!(
            period_start_date(PeriodKind::Monthly, date),
            NaiveDate::from_ymd_opt(2025, 6, 1).unwrap()
        );
    }
}
