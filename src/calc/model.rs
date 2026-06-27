use chrono::{Datelike, Duration, NaiveDate};

use super::constants::{ALPHA12, ALPHA26, ALPHA9};

#[derive(Debug, Clone)]
pub(super) struct SourceBar {
    pub(super) ts_code: String,
    pub(super) trade_date: NaiveDate,
    pub(super) open_qfq: Option<f64>,
    pub(super) high_qfq: Option<f64>,
    pub(super) low_qfq: Option<f64>,
    pub(super) close_qfq: Option<f64>,
    pub(super) vol: Option<f64>,
    pub(super) turnover_rate: Option<f64>,
}

#[derive(Debug, Default)]
pub(super) struct ComputedRows {
    pub(super) period_states: Vec<PeriodStateRow>,
    pub(super) index_period_states: Vec<PeriodStateRow>,
    pub(super) asof_rows: Vec<AsofRow>,
    pub(super) index_asof_rows: Vec<AsofRow>,
    pub(super) rolling_rows: Vec<RollingRow>,
    pub(super) left_peak_rows: Vec<LeftPeakRow>,
}

impl ComputedRows {
    pub(super) fn append(&mut self, mut other: ComputedRows) {
        self.period_states.append(&mut other.period_states);
        self.index_period_states
            .append(&mut other.index_period_states);
        self.asof_rows.append(&mut other.asof_rows);
        self.index_asof_rows.append(&mut other.index_asof_rows);
        self.rolling_rows.append(&mut other.rolling_rows);
        self.left_peak_rows.append(&mut other.left_peak_rows);
    }
}

#[derive(Debug)]
pub(super) struct PeriodStateRow {
    pub(super) ts_code: String,
    pub(super) period_type: String,
    pub(super) period_start_date: NaiveDate,
    pub(super) period_end_date: NaiveDate,
    pub(super) period_close_qfq: Option<f64>,
    pub(super) ema12: Option<f64>,
    pub(super) ema26: Option<f64>,
    pub(super) dif: Option<f64>,
    pub(super) dea: Option<f64>,
    pub(super) hist_x2: Option<f64>,
    pub(super) period_count: Option<i32>,
}

impl PeriodStateRow {
    pub(super) fn from_state(
        ts_code: &str,
        period_type: &str,
        period_start_date: NaiveDate,
        period_end_date: NaiveDate,
        period_close_qfq: f64,
        state: MacdState,
    ) -> Self {
        Self {
            ts_code: ts_code.to_string(),
            period_type: period_type.to_string(),
            period_start_date,
            period_end_date,
            period_close_qfq: Some(period_close_qfq),
            ema12: Some(state.ema12),
            ema26: Some(state.ema26),
            dif: Some(state.dif),
            dea: Some(state.dea),
            hist_x2: Some(state.hist_x2),
            period_count: Some(state.period_count),
        }
    }
}

#[derive(Debug)]
pub(super) struct AsofRow {
    pub(super) ts_code: String,
    pub(super) trade_date: NaiveDate,
    pub(super) daily_dif_asof: Option<f64>,
    pub(super) daily_dea_asof: Option<f64>,
    pub(super) daily_hist_x2_asof: Option<f64>,
    pub(super) daily_dea_pctile_asof: Option<f64>,
    pub(super) daily_period_count: Option<i32>,
    pub(super) weekly_dif_asof: Option<f64>,
    pub(super) weekly_dea_asof: Option<f64>,
    pub(super) weekly_hist_x2_asof: Option<f64>,
    pub(super) weekly_dea_pctile_asof: Option<f64>,
    pub(super) weekly_period_count: Option<i32>,
    pub(super) monthly_dif_asof: Option<f64>,
    pub(super) monthly_dea_asof: Option<f64>,
    pub(super) monthly_hist_x2_asof: Option<f64>,
    pub(super) monthly_dea_pctile_asof: Option<f64>,
    pub(super) monthly_period_count: Option<i32>,
}

#[derive(Debug)]
pub(super) struct RollingRow {
    pub(super) ts_code: String,
    pub(super) trade_date: NaiveDate,
    pub(super) ma25_qfq: Option<f64>,
    pub(super) ma144_qfq: Option<f64>,
    pub(super) ma220_qfq: Option<f64>,
    pub(super) high_20_qfq: Option<f64>,
    pub(super) high_90_qfq: Option<f64>,
    pub(super) low_90_qfq: Option<f64>,
    pub(super) high_120_qfq: Option<f64>,
    pub(super) low_120_qfq: Option<f64>,
    pub(super) close_to_20d_max_close_pct: Option<f64>,
    pub(super) position_90d: Option<f64>,
    pub(super) position_120d: Option<f64>,
    pub(super) volume_ma5: Option<f64>,
    pub(super) volume_ma20: Option<f64>,
    pub(super) volume_to_ma5_ratio: Option<f64>,
    pub(super) volume_to_ma20_ratio: Option<f64>,
    pub(super) volume_ma5_to_ma20_ratio: Option<f64>,
    pub(super) turnover_rate_ma5: Option<f64>,
    pub(super) turnover_to_ma5_ratio: Option<f64>,
    pub(super) range_compression_20d: Option<f64>,
    pub(super) range_compression_40d: Option<f64>,
}

#[derive(Debug)]
pub(super) struct LeftPeakRow {
    pub(super) ts_code: String,
    pub(super) trade_date: NaiveDate,
    pub(super) left_peak_date: Option<NaiveDate>,
    pub(super) left_peak_high: Option<f64>,
    pub(super) breakout_date: Option<NaiveDate>,
    pub(super) breakout_close: Option<f64>,
    pub(super) breakout_body_above_left_peak: Option<bool>,
    pub(super) first_bear_date: Option<NaiveDate>,
    pub(super) first_bear_open: Option<f64>,
    pub(super) first_bear_missing: Option<bool>,
    pub(super) pick_close: Option<f64>,
    pub(super) b_div_a: Option<f64>,
    pub(super) abs_ba_minus_1: Option<f64>,
    pub(super) a_lt_b: Option<bool>,
    pub(super) is_valid: Option<bool>,
    pub(super) status: String,
}

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct MacdAccumulator {
    pub(super) state: Option<MacdState>,
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
pub(super) enum PeriodKey {
    Weekly { year: i32, week: u32 },
    Monthly { year: i32, month: u32 },
}

#[derive(Debug)]
pub(super) struct PeriodBuilder {
    pub(super) kind: PeriodKind,
    pub(super) key: PeriodKey,
    pub(super) start_date: NaiveDate,
    pub(super) end_date: NaiveDate,
    pub(super) close_qfq: Option<f64>,
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
