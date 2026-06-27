use chrono::NaiveDate;

use super::{
    constants::{BREAKOUT_LOOKBACK, HIGH_CYCLE_DAYS, MIN_PULLBACK_PCT, SWING_LOOKBACK},
    model::{LeftPeakRow, SourceBar},
};

pub(super) fn compute_left_peak_row(bars: &[SourceBar], idx: usize) -> LeftPeakRow {
    let frame = bars[..=idx]
        .iter()
        .filter(|bar| {
            bar.open_qfq.is_some()
                && bar.high_qfq.is_some()
                && bar.low_qfq.is_some()
                && bar.close_qfq.is_some()
        })
        .cloned()
        .collect::<Vec<_>>();
    let target = &bars[idx];
    let pick_close = target.close_qfq;
    if frame.len() < 20.max(BREAKOUT_LOOKBACK + SWING_LOOKBACK * 2 + 1) {
        return LeftPeakRow::empty(target, pick_close, "insufficient_history");
    }

    let recent_start = frame.len().saturating_sub(BREAKOUT_LOOKBACK);
    let prior_end = recent_start;
    if prior_end == 0 {
        return LeftPeakRow::empty(target, pick_close, "insufficient_history");
    }
    let peaks = swing_highs(&frame[..prior_end]);
    if peaks.is_empty() {
        return LeftPeakRow::empty(target, pick_close, "no_swing_high");
    }
    let cycle_start = frame.len().saturating_sub(HIGH_CYCLE_DAYS);
    let left_peak_idx =
        peaks
            .into_iter()
            .filter(|idx| *idx >= cycle_start)
            .max_by(|left, right| {
                high(&frame[*left])
                    .partial_cmp(&high(&frame[*right]))
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| left.cmp(right))
            });
    let Some(left_peak_idx) = left_peak_idx else {
        return LeftPeakRow::empty(target, pick_close, "no_cycle_peak");
    };
    let left_peak = &frame[left_peak_idx];
    let left_peak_high = high(left_peak);
    if left_peak_high <= 0.0 {
        return LeftPeakRow::with_peak(
            target,
            pick_close,
            &frame,
            left_peak,
            None,
            "no_close_breakout",
        );
    }

    let breakout_idx = (recent_start..frame.len()).find(|idx| close(&frame[*idx]) > left_peak_high);
    let Some(breakout_idx) = breakout_idx else {
        return LeftPeakRow::with_peak(
            target,
            pick_close,
            &frame,
            left_peak,
            None,
            "no_close_breakout",
        );
    };
    let breakout = &frame[breakout_idx];
    let body_window_start = breakout_idx.saturating_sub(BREAKOUT_LOOKBACK - 1);
    let body_above = frame[body_window_start..=breakout_idx]
        .iter()
        .any(|bar| open(bar) > left_peak_high && close(bar) > left_peak_high);
    if !body_above {
        return LeftPeakRow::with_peak_and_breakout(
            target,
            pick_close,
            &frame,
            left_peak,
            breakout,
            Some(false),
            "no_body_breakout",
        );
    }

    let min_low = frame[left_peak_idx + 1..=breakout_idx]
        .iter()
        .map(low)
        .reduce(f64::min);
    let Some(min_low) = min_low else {
        return LeftPeakRow::with_peak_and_breakout(
            target,
            pick_close,
            &frame,
            left_peak,
            breakout,
            Some(true),
            "pullback_too_small",
        );
    };
    let pullback_pct = (left_peak_high - min_low) / left_peak_high * 100.0;
    if pullback_pct < MIN_PULLBACK_PCT {
        return LeftPeakRow::with_peak_and_breakout(
            target,
            pick_close,
            &frame,
            left_peak,
            breakout,
            Some(true),
            "pullback_too_small",
        );
    }

    LeftPeakRow::with_peak_and_breakout(
        target,
        pick_close,
        &frame,
        left_peak,
        breakout,
        Some(true),
        "valid",
    )
}

fn swing_highs(frame: &[SourceBar]) -> Vec<usize> {
    if frame.len() < SWING_LOOKBACK * 2 + 1 {
        return Vec::new();
    }
    let mut peaks = Vec::new();
    for idx in SWING_LOOKBACK..frame.len() - SWING_LOOKBACK {
        let value = high(&frame[idx]);
        let left_max = frame[idx - SWING_LOOKBACK..idx]
            .iter()
            .map(high)
            .fold(f64::NEG_INFINITY, f64::max);
        let right_max = frame[idx + 1..=idx + SWING_LOOKBACK]
            .iter()
            .map(high)
            .fold(f64::NEG_INFINITY, f64::max);
        if value > left_max && value >= right_max {
            peaks.push(idx);
        }
    }
    peaks
}

impl LeftPeakRow {
    pub(super) fn empty(target: &SourceBar, pick_close: Option<f64>, status: &str) -> Self {
        Self {
            ts_code: target.ts_code.clone(),
            trade_date: target.trade_date,
            left_peak_date: None,
            left_peak_high: None,
            breakout_date: None,
            breakout_close: None,
            breakout_body_above_left_peak: None,
            first_bear_date: None,
            first_bear_open: None,
            first_bear_missing: None,
            pick_close,
            b_div_a: None,
            abs_ba_minus_1: None,
            a_lt_b: None,
            is_valid: Some(false),
            status: status.to_string(),
        }
    }

    pub(super) fn with_peak(
        target: &SourceBar,
        pick_close: Option<f64>,
        frame: &[SourceBar],
        left_peak: &SourceBar,
        breakout: Option<&SourceBar>,
        status: &str,
    ) -> Self {
        let (first_bear_date, first_bear_open, first_bear_missing) =
            first_bear_after(frame, left_peak.trade_date, target.trade_date);
        let anchor = first_bear_open.and_then(|anchor| derive_anchor_values(pick_close, anchor));
        Self {
            ts_code: target.ts_code.clone(),
            trade_date: target.trade_date,
            left_peak_date: Some(left_peak.trade_date),
            left_peak_high: left_peak.high_qfq,
            breakout_date: breakout.map(|bar| bar.trade_date),
            breakout_close: breakout.and_then(|bar| bar.close_qfq),
            breakout_body_above_left_peak: None,
            first_bear_date,
            first_bear_open,
            first_bear_missing: Some(first_bear_missing),
            pick_close,
            b_div_a: anchor.map(|values| values.0),
            abs_ba_minus_1: anchor.map(|values| values.1),
            a_lt_b: anchor.map(|values| values.2),
            is_valid: Some(false),
            status: status.to_string(),
        }
    }

    pub(super) fn with_peak_and_breakout(
        target: &SourceBar,
        pick_close: Option<f64>,
        frame: &[SourceBar],
        left_peak: &SourceBar,
        breakout: &SourceBar,
        body_above: Option<bool>,
        status: &str,
    ) -> Self {
        let mut row = Self::with_peak(target, pick_close, frame, left_peak, Some(breakout), status);
        row.breakout_body_above_left_peak = body_above;
        row.is_valid = Some(status == "valid");
        row
    }
}

fn first_bear_after(
    frame: &[SourceBar],
    left_peak_date: NaiveDate,
    target_date: NaiveDate,
) -> (Option<NaiveDate>, Option<f64>, bool) {
    let first_bear = frame.iter().find(|bar| {
        bar.trade_date > left_peak_date && bar.trade_date <= target_date && close(bar) < open(bar)
    });
    match first_bear {
        Some(bar) => (Some(bar.trade_date), bar.open_qfq, false),
        None => (None, None, true),
    }
}

fn derive_anchor_values(pick_close: Option<f64>, anchor_open: f64) -> Option<(f64, f64, bool)> {
    let pick_close = pick_close?;
    if anchor_open == 0.0 {
        return None;
    }
    let b_div_a = pick_close / anchor_open;
    Some((b_div_a, (b_div_a - 1.0).abs(), anchor_open < pick_close))
}

fn open(bar: &SourceBar) -> f64 {
    bar.open_qfq.expect("left peak frame filters open_qfq")
}

fn high(bar: &SourceBar) -> f64 {
    bar.high_qfq.expect("left peak frame filters high_qfq")
}

fn low(bar: &SourceBar) -> f64 {
    bar.low_qfq.expect("left peak frame filters low_qfq")
}

fn close(bar: &SourceBar) -> f64 {
    bar.close_qfq.expect("left peak frame filters close_qfq")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_left_peak_valid_breakout_with_anchor() {
        let mut bars = Vec::new();
        for day in 1..=25 {
            let (open, high, low, close) = match day {
                5 => (9.8, 12.0, 9.5, 11.0),
                6 => (10.8, 11.0, 10.0, 10.2),
                20 => (11.0, 11.5, 10.5, 11.2),
                21 => (11.4, 11.8, 10.8, 11.5),
                22 => (12.1, 12.8, 11.9, 12.4),
                23 => (12.2, 12.5, 11.8, 12.3),
                24 => (12.0, 12.6, 11.7, 12.4),
                25 => (12.2, 12.7, 12.0, 12.5),
                _ => (9.5, 10.0, 9.0, 9.6),
            };
            bars.push(SourceBar {
                ts_code: "000001.SZ".to_string(),
                trade_date: NaiveDate::from_ymd_opt(2025, 1, day).unwrap(),
                open_qfq: Some(open),
                high_qfq: Some(high),
                low_qfq: Some(low),
                close_qfq: Some(close),
                vol: Some(1.0),
                turnover_rate: Some(1.0),
            });
        }

        let row = compute_left_peak_row(&bars, 24);
        assert_eq!(row.status, "valid");
        assert_eq!(
            row.left_peak_date,
            Some(NaiveDate::from_ymd_opt(2025, 1, 5).unwrap())
        );
        assert_eq!(
            row.first_bear_date,
            Some(NaiveDate::from_ymd_opt(2025, 1, 6).unwrap())
        );
        assert_eq!(row.first_bear_open, Some(10.8));
        assert_eq!(row.first_bear_missing, Some(false));
    }
}
