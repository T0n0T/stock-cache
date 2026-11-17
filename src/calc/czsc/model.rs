use chrono::NaiveDate;

#[derive(Debug, Clone, Copy)]
pub(crate) struct CzscConfig {
    pub(crate) min_bi_len: usize,
    pub(crate) max_bi_num: usize,
}

#[derive(Debug, Clone, Copy)]
pub(in crate::calc) struct CzscBar {
    pub(in crate::calc) trade_date: NaiveDate,
    pub(in crate::calc) open: f64,
    pub(in crate::calc) high: f64,
    pub(in crate::calc) low: f64,
    pub(in crate::calc) close: f64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CzscSourceBar {
    pub(crate) trade_date: NaiveDate,
    pub(crate) open: Option<f64>,
    pub(crate) high: Option<f64>,
    pub(crate) low: Option<f64>,
    pub(crate) close: Option<f64>,
}

impl CzscSourceBar {
    pub(super) fn valid_bar(self) -> Option<CzscBar> {
        let bar = CzscBar {
            trade_date: self.trade_date,
            open: self.open?,
            high: self.high?,
            low: self.low?,
            close: self.close?,
        };
        bar.is_valid().then_some(bar)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CzscSnapshot {
    pub(crate) source_bar_valid: bool,
    pub(crate) status: String,
    pub(crate) available_finished_bi_count: usize,
    pub(crate) recent_bi_count: usize,
    pub(crate) last_bi_direction: Option<String>,
    pub(crate) last_bi_start_date: Option<NaiveDate>,
    pub(crate) last_bi_end_date: Option<NaiveDate>,
    pub(crate) last_bi_high: Option<f64>,
    pub(crate) last_bi_low: Option<f64>,
    pub(crate) last_bi_mid: Option<f64>,
    pub(crate) latest_zs_start_date: Option<NaiveDate>,
    pub(crate) latest_zs_end_date: Option<NaiveDate>,
    pub(crate) latest_zs_bi_count: Option<usize>,
    pub(crate) latest_zs_zg: Option<f64>,
    pub(crate) latest_zs_zd: Option<f64>,
    pub(crate) latest_zs_zz: Option<f64>,
    pub(crate) recent_peak_date: Option<NaiveDate>,
    pub(crate) recent_peak_high: Option<f64>,
    pub(crate) post_peak_first_bear_date: Option<NaiveDate>,
    pub(crate) post_peak_first_bear_open: Option<f64>,
    pub(crate) post_peak_first_bear_high: Option<f64>,
    pub(crate) post_peak_first_bear_low: Option<f64>,
    pub(crate) post_peak_first_bear_close: Option<f64>,
    pub(crate) post_peak_first_bear_missing: Option<bool>,
}

impl CzscBar {
    pub(in crate::calc) fn is_valid_ohlc(open: f64, high: f64, low: f64, close: f64) -> bool {
        [open, high, low, close]
            .into_iter()
            .all(|value| value.is_finite() && value > 0.0)
            && high >= open.max(close)
            && low <= open.min(close)
            && high >= low
    }

    pub(in crate::calc) fn is_valid(self) -> bool {
        Self::is_valid_ohlc(self.open, self.high, self.low, self.close)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Direction {
    Up,
    Down,
}

impl Direction {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mark {
    Bottom,
    Top,
}

#[derive(Debug, Clone)]
pub(super) struct NewBar {
    pub(super) trade_date: NaiveDate,
    pub(super) high: f64,
    pub(super) low: f64,
    pub(super) raw_indices: Vec<usize>,
}

impl NewBar {
    pub(super) fn from_raw(raw_index: usize, bar: CzscBar) -> Self {
        Self {
            trade_date: bar.trade_date,
            high: bar.high,
            low: bar.low,
            raw_indices: vec![raw_index],
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct Fx {
    pub(super) trade_date: NaiveDate,
    pub(super) mark: Mark,
    pub(super) high: f64,
    pub(super) low: f64,
    pub(super) value: f64,
    pub(super) first_date: NaiveDate,
    pub(super) last_date: NaiveDate,
}

#[derive(Debug, Clone)]
pub(super) struct Bi {
    pub(super) fx_a: Fx,
    pub(super) fx_b: Fx,
    pub(super) direction: Direction,
    pub(super) bars: Vec<NewBar>,
}

impl Bi {
    pub(super) fn high(&self) -> f64 {
        self.fx_a.high.max(self.fx_b.high)
    }

    pub(super) fn low(&self) -> f64 {
        self.fx_a.low.min(self.fx_b.low)
    }

    #[cfg(test)]
    pub(super) fn summary(&self) -> BiSummary {
        BiSummary {
            direction: self.direction,
            start_date: self.fx_a.trade_date,
            end_date: self.fx_b.trade_date,
            high: self.high(),
            low: self.low(),
        }
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub(super) struct BiSummary {
    pub(super) direction: Direction,
    pub(super) start_date: NaiveDate,
    pub(super) end_date: NaiveDate,
    pub(super) high: f64,
    pub(super) low: f64,
}
