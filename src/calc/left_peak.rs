//! Daily left-peak/breakout factors for the v3 calculation contract.

use anyhow::{ensure, Result};
use chrono::NaiveDate;
use serde::Serialize;

const SWING_LOOKBACK: usize = 2;
const BREAKOUT_LOOKBACK: usize = 5;
const HIGH_CYCLE_DAYS: usize = 360;
const MIN_PULLBACK_PCT: f64 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LeftPeakBar {
    pub trade_date: NaiveDate,
    pub open: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub close: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LeftPeakValue {
    pub trade_date: NaiveDate,
    pub left_peak_date: Option<NaiveDate>,
    pub left_peak_high: Option<f64>,
    pub breakout_date: Option<NaiveDate>,
    pub breakout_close: Option<f64>,
    pub breakout_body_above_left_peak: Option<bool>,
    pub first_bear_date: Option<NaiveDate>,
    pub first_bear_open: Option<f64>,
    pub first_bear_missing: Option<bool>,
    pub pick_close: Option<f64>,
    pub b_div_a: Option<f64>,
    pub abs_ba_minus_1: Option<f64>,
    pub a_lt_b: Option<bool>,
    pub is_valid: Option<bool>,
    pub status: String,
}

pub fn calculate_left_peak(bars: &[LeftPeakBar]) -> Result<Vec<LeftPeakValue>> {
    validate_dates(bars)?;
    Ok((0..bars.len()).map(|idx| compute_row(bars, idx)).collect())
}

fn compute_row(bars: &[LeftPeakBar], idx: usize) -> LeftPeakValue {
    let frame = bars[..=idx]
        .iter()
        .copied()
        .filter(valid_bar)
        .collect::<Vec<_>>();
    let target = bars[idx];
    let pick_close = target.close;
    if frame.len() < 20.max(BREAKOUT_LOOKBACK + SWING_LOOKBACK * 2 + 1) {
        return empty(target, pick_close, "insufficient_history");
    }
    let recent_start = frame.len().saturating_sub(BREAKOUT_LOOKBACK);
    let peaks = swing_highs(&frame[..recent_start]);
    if peaks.is_empty() {
        return empty(target, pick_close, "no_swing_high");
    }
    let cycle_start = frame.len().saturating_sub(HIGH_CYCLE_DAYS);
    let left_peak_idx = peaks
        .into_iter()
        .filter(|peak| *peak >= cycle_start)
        .max_by(|left, right| {
            high(frame[*left])
                .partial_cmp(&high(frame[*right]))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.cmp(right))
        });
    let Some(left_peak_idx) = left_peak_idx else {
        return empty(target, pick_close, "no_cycle_peak");
    };
    let left_peak = frame[left_peak_idx];
    let left_peak_high = high(left_peak);
    if left_peak_high <= 0.0 {
        return with_peak(
            target,
            pick_close,
            &frame,
            left_peak,
            None,
            None,
            "no_close_breakout",
        );
    }
    let breakout_idx =
        (recent_start..frame.len()).find(|position| close(frame[*position]) > left_peak_high);
    let Some(breakout_idx) = breakout_idx else {
        return with_peak(
            target,
            pick_close,
            &frame,
            left_peak,
            None,
            None,
            "no_close_breakout",
        );
    };
    let breakout = frame[breakout_idx];
    let body_window_start = breakout_idx.saturating_sub(BREAKOUT_LOOKBACK - 1);
    let body_above = frame[body_window_start..=breakout_idx]
        .iter()
        .any(|bar| open(*bar) > left_peak_high && close(*bar) > left_peak_high);
    if !body_above {
        return with_peak(
            target,
            pick_close,
            &frame,
            left_peak,
            Some(breakout),
            Some(false),
            "no_body_breakout",
        );
    }
    let min_low = frame[left_peak_idx + 1..=breakout_idx]
        .iter()
        .map(|bar| low(*bar))
        .reduce(f64::min);
    let Some(min_low) = min_low else {
        return with_peak(
            target,
            pick_close,
            &frame,
            left_peak,
            Some(breakout),
            Some(true),
            "pullback_too_small",
        );
    };
    if (left_peak_high - min_low) / left_peak_high * 100.0 < MIN_PULLBACK_PCT {
        return with_peak(
            target,
            pick_close,
            &frame,
            left_peak,
            Some(breakout),
            Some(true),
            "pullback_too_small",
        );
    }
    let mut row = with_peak(
        target,
        pick_close,
        &frame,
        left_peak,
        Some(breakout),
        Some(true),
        "valid",
    );
    row.is_valid = Some(true);
    row
}

fn valid_bar(bar: &LeftPeakBar) -> bool {
    bar.open.is_some() && bar.high.is_some() && bar.low.is_some() && bar.close.is_some()
}
fn swing_highs(frame: &[LeftPeakBar]) -> Vec<usize> {
    if frame.len() < SWING_LOOKBACK * 2 + 1 {
        return Vec::new();
    }
    (SWING_LOOKBACK..frame.len() - SWING_LOOKBACK)
        .filter(|idx| {
            let value = high(frame[*idx]);
            let left_max = frame[idx - SWING_LOOKBACK..*idx]
                .iter()
                .map(|bar| high(*bar))
                .fold(f64::NEG_INFINITY, f64::max);
            let right_max = frame[*idx + 1..=*idx + SWING_LOOKBACK]
                .iter()
                .map(|bar| high(*bar))
                .fold(f64::NEG_INFINITY, f64::max);
            value > left_max && value >= right_max
        })
        .collect()
}
fn empty(target: LeftPeakBar, pick_close: Option<f64>, status: &str) -> LeftPeakValue {
    LeftPeakValue {
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
fn with_peak(
    target: LeftPeakBar,
    pick_close: Option<f64>,
    frame: &[LeftPeakBar],
    left_peak: LeftPeakBar,
    breakout: Option<LeftPeakBar>,
    body_above: Option<bool>,
    status: &str,
) -> LeftPeakValue {
    let (first_bear_date, first_bear_open, first_bear_missing) =
        first_bear_after(frame, left_peak.trade_date, target.trade_date);
    let anchor = first_bear_open.and_then(|open| derive_anchor_values(pick_close, open));
    LeftPeakValue {
        trade_date: target.trade_date,
        left_peak_date: Some(left_peak.trade_date),
        left_peak_high: left_peak.high,
        breakout_date: breakout.map(|bar| bar.trade_date),
        breakout_close: breakout.and_then(|bar| bar.close),
        breakout_body_above_left_peak: body_above,
        first_bear_date,
        first_bear_open,
        first_bear_missing: Some(first_bear_missing),
        pick_close,
        b_div_a: anchor.map(|v| v.0),
        abs_ba_minus_1: anchor.map(|v| v.1),
        a_lt_b: anchor.map(|v| v.2),
        is_valid: Some(status == "valid"),
        status: status.to_string(),
    }
}
fn first_bear_after(
    frame: &[LeftPeakBar],
    left_peak_date: NaiveDate,
    target_date: NaiveDate,
) -> (Option<NaiveDate>, Option<f64>, bool) {
    match frame.iter().find(|bar| {
        bar.trade_date > left_peak_date
            && bar.trade_date <= target_date
            && close(**bar) < open(**bar)
    }) {
        Some(bar) => (Some(bar.trade_date), bar.open, false),
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
fn open(bar: LeftPeakBar) -> f64 {
    bar.open.expect("left peak frame filters open")
}
fn high(bar: LeftPeakBar) -> f64 {
    bar.high.expect("left peak frame filters high")
}
fn low(bar: LeftPeakBar) -> f64 {
    bar.low.expect("left peak frame filters low")
}
fn close(bar: LeftPeakBar) -> f64 {
    bar.close.expect("left peak frame filters close")
}
fn validate_dates(bars: &[LeftPeakBar]) -> Result<()> {
    for pair in bars.windows(2) {
        ensure!(
            pair[0].trade_date < pair[1].trade_date,
            "left peak bars must be strictly increasing"
        );
    }
    for bar in bars {
        for value in [bar.open, bar.high, bar.low, bar.close]
            .into_iter()
            .flatten()
        {
            ensure!(
                value.is_finite(),
                "left peak input contains non-finite value"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finds_peak_breakout_and_anchor() {
        let bars = (1..=25)
            .map(|day| {
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
                LeftPeakBar {
                    trade_date: NaiveDate::from_ymd_opt(2025, 1, day).unwrap(),
                    open: Some(open),
                    high: Some(high),
                    low: Some(low),
                    close: Some(close),
                }
            })
            .collect::<Vec<_>>();
        let rows = calculate_left_peak(&bars).unwrap();
        assert_eq!(rows[24].status, "valid");
        assert_eq!(
            rows[24].left_peak_date,
            Some(NaiveDate::from_ymd_opt(2025, 1, 5).unwrap())
        );
        assert_eq!(
            rows[24].first_bear_date,
            Some(NaiveDate::from_ymd_opt(2025, 1, 6).unwrap())
        );
    }
}
