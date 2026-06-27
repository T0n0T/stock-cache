use std::{collections::HashSet, thread};

use chrono::NaiveDate;

use super::{
    constants::{BREAKOUT_LOOKBACK, HIGH_CYCLE_DAYS, MIN_PULLBACK_PCT, SWING_LOOKBACK},
    model::{
        AsofRow, ComputedRows, LeftPeakRow, MacdAccumulator, MacdState, PeriodBuilder, PeriodKind,
        PeriodStateRow, RollingRow, SourceBar,
    },
};

pub(super) fn distinct_trade_dates(source_rows: &[SourceBar]) -> Vec<NaiveDate> {
    let mut dates = source_rows
        .iter()
        .map(|row| row.trade_date)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    dates.sort_unstable();
    dates
}

pub(super) fn compute_calc_rows(
    source_rows: &[SourceBar],
    index_source_rows: &[SourceBar],
    target_dates: &[NaiveDate],
    max_concurrency: u32,
) -> ComputedRows {
    let stock_ranges_for_source = stock_ranges(source_rows);
    let index_ranges = stock_ranges(index_source_rows);
    let concurrency = (max_concurrency.max(1) as usize).min(stock_ranges_for_source.len().max(1));
    let mut computed = if concurrency <= 1 || stock_ranges_for_source.len() <= 1 {
        compute_calc_rows_for_ranges(source_rows, &stock_ranges_for_source, target_dates)
    } else {
        let chunk_size = stock_ranges_for_source.len().div_ceil(concurrency);
        let chunks = stock_ranges_for_source
            .chunks(chunk_size)
            .collect::<Vec<_>>();
        eprintln!(
            "[calc] compute rows parallel stocks={} workers={}",
            stock_ranges_for_source.len(),
            chunks.len()
        );

        thread::scope(|scope| {
            let handles = chunks
                .into_iter()
                .map(|chunk| {
                    scope.spawn(move || {
                        compute_calc_rows_for_ranges(source_rows, chunk, target_dates)
                    })
                })
                .collect::<Vec<_>>();
            let mut merged = ComputedRows::default();
            for handle in handles {
                merged.append(handle.join().expect("calc worker thread panicked"));
            }
            merged
        })
    };
    compute_index_asof_rows(
        index_source_rows,
        &index_ranges,
        target_dates,
        &mut computed,
    );
    computed
}

fn compute_index_asof_rows(
    source_rows: &[SourceBar],
    ranges: &[(usize, usize)],
    target_dates: &[NaiveDate],
    computed: &mut ComputedRows,
) {
    let target_dates = target_dates.iter().copied().collect::<HashSet<_>>();
    for (start, end) in ranges {
        compute_index_rows(&source_rows[*start..*end], &target_dates, computed);
    }
}

pub(super) fn compute_calc_rows_for_ranges(
    source_rows: &[SourceBar],
    stock_ranges: &[(usize, usize)],
    target_dates: &[NaiveDate],
) -> ComputedRows {
    let target_dates = target_dates.iter().copied().collect::<HashSet<_>>();
    let mut computed = ComputedRows::default();
    for (start, end) in stock_ranges {
        compute_stock_rows(&source_rows[*start..*end], &target_dates, &mut computed);
    }
    computed
}

fn stock_ranges(source_rows: &[SourceBar]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut offset = 0;
    while offset < source_rows.len() {
        let ts_code = &source_rows[offset].ts_code;
        let start = offset;
        while offset < source_rows.len() && source_rows[offset].ts_code == *ts_code {
            offset += 1;
        }
        ranges.push((start, offset));
    }
    ranges
}

fn compute_stock_rows(
    bars: &[SourceBar],
    target_dates: &HashSet<NaiveDate>,
    computed: &mut ComputedRows,
) {
    let Some(first) = bars.first() else {
        return;
    };
    let mut daily_macd = MacdAccumulator::default();
    let mut weekly_macd = MacdAccumulator::default();
    let mut monthly_macd = MacdAccumulator::default();
    let mut current_week = PeriodBuilder::new(PeriodKind::Weekly, first.trade_date);
    let mut current_month = PeriodBuilder::new(PeriodKind::Monthly, first.trade_date);
    let mut daily_deas = Vec::new();
    let mut weekly_deas = Vec::new();
    let mut monthly_deas = Vec::new();

    for (idx, bar) in bars.iter().enumerate() {
        if !current_week.contains(bar.trade_date) {
            finalize_period(
                &bars[0].ts_code,
                "weekly",
                &current_week,
                &mut weekly_macd,
                &mut weekly_deas,
                &mut computed.period_states,
            );
            current_week = PeriodBuilder::new(PeriodKind::Weekly, bar.trade_date);
        }
        if !current_month.contains(bar.trade_date) {
            finalize_period(
                &bars[0].ts_code,
                "monthly",
                &current_month,
                &mut monthly_macd,
                &mut monthly_deas,
                &mut computed.period_states,
            );
            current_month = PeriodBuilder::new(PeriodKind::Monthly, bar.trade_date);
        }

        let daily_state = bar.close_qfq.map(|close| daily_macd.push(close));
        if let Some(state) = daily_state {
            daily_deas.push(state.dea);
            computed.period_states.push(PeriodStateRow::from_state(
                &bar.ts_code,
                "daily",
                bar.trade_date,
                bar.trade_date,
                close_or_nan(bar.close_qfq),
                state,
            ));
        }
        current_week.observe(bar);
        current_month.observe(bar);

        if target_dates.contains(&bar.trade_date) {
            computed.asof_rows.push(compute_asof_row(
                bar,
                daily_state,
                &daily_deas,
                &weekly_macd,
                &weekly_deas,
                &monthly_macd,
                &monthly_deas,
            ));
            computed.rolling_rows.push(compute_rolling_row(bars, idx));
            computed
                .left_peak_rows
                .push(compute_left_peak_row(bars, idx));
        }
    }
}

fn compute_index_rows(
    bars: &[SourceBar],
    target_dates: &HashSet<NaiveDate>,
    computed: &mut ComputedRows,
) {
    let Some(first) = bars.first() else {
        return;
    };
    let mut daily_macd = MacdAccumulator::default();
    let mut weekly_macd = MacdAccumulator::default();
    let mut monthly_macd = MacdAccumulator::default();
    let mut current_week = PeriodBuilder::new(PeriodKind::Weekly, first.trade_date);
    let mut current_month = PeriodBuilder::new(PeriodKind::Monthly, first.trade_date);
    let mut daily_deas = Vec::new();
    let mut weekly_deas = Vec::new();
    let mut monthly_deas = Vec::new();

    for bar in bars {
        if !current_week.contains(bar.trade_date) {
            finalize_period(
                &bars[0].ts_code,
                "weekly",
                &current_week,
                &mut weekly_macd,
                &mut weekly_deas,
                &mut computed.index_period_states,
            );
            current_week = PeriodBuilder::new(PeriodKind::Weekly, bar.trade_date);
        }
        if !current_month.contains(bar.trade_date) {
            finalize_period(
                &bars[0].ts_code,
                "monthly",
                &current_month,
                &mut monthly_macd,
                &mut monthly_deas,
                &mut computed.index_period_states,
            );
            current_month = PeriodBuilder::new(PeriodKind::Monthly, bar.trade_date);
        }

        let daily_state = bar.close_qfq.map(|close| daily_macd.push(close));
        if let Some(state) = daily_state {
            daily_deas.push(state.dea);
            computed
                .index_period_states
                .push(PeriodStateRow::from_state(
                    &bar.ts_code,
                    "daily",
                    bar.trade_date,
                    bar.trade_date,
                    close_or_nan(bar.close_qfq),
                    state,
                ));
        }
        current_week.observe(bar);
        current_month.observe(bar);

        if target_dates.contains(&bar.trade_date) {
            computed.index_asof_rows.push(compute_asof_row(
                bar,
                daily_state,
                &daily_deas,
                &weekly_macd,
                &weekly_deas,
                &monthly_macd,
                &monthly_deas,
            ));
        }
    }
}

fn finalize_period(
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

fn compute_asof_row(
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

fn compute_rolling_row(bars: &[SourceBar], idx: usize) -> RollingRow {
    let bar = &bars[idx];
    let close = bar.close_qfq;
    let high_20 = rolling_extreme(bars, idx, 20, PriceField::High, Extreme::Max);
    let low_20 = rolling_extreme(bars, idx, 20, PriceField::Low, Extreme::Min);
    let high_90 = rolling_extreme(bars, idx, 90, PriceField::High, Extreme::Max);
    let low_90 = rolling_extreme(bars, idx, 90, PriceField::Low, Extreme::Min);
    let high_120 = rolling_extreme(bars, idx, 120, PriceField::High, Extreme::Max);
    let low_120 = rolling_extreme(bars, idx, 120, PriceField::Low, Extreme::Min);
    let max_close_20 = rolling_extreme(bars, idx, 20, PriceField::Close, Extreme::Max);
    let volume_ma5 = rolling_average(bars, idx, 5, PriceField::Vol);
    let volume_ma20 = rolling_average(bars, idx, 20, PriceField::Vol);
    let turnover_rate_ma5 = rolling_average(bars, idx, 5, PriceField::TurnoverRate);
    RollingRow {
        ts_code: bar.ts_code.clone(),
        trade_date: bar.trade_date,
        ma25_qfq: rolling_average(bars, idx, 25, PriceField::Close),
        ma144_qfq: rolling_average(bars, idx, 144, PriceField::Close),
        ma220_qfq: rolling_average(bars, idx, 220, PriceField::Close),
        high_20_qfq: high_20,
        high_90_qfq: high_90,
        low_90_qfq: low_90,
        high_120_qfq: high_120,
        low_120_qfq: low_120,
        close_to_20d_max_close_pct: ratio_minus_one(close, max_close_20),
        position_90d: position(close, low_90, high_90),
        position_120d: position(close, low_120, high_120),
        volume_ma5,
        volume_ma20,
        volume_to_ma5_ratio: ratio(bar.vol, volume_ma5),
        volume_to_ma20_ratio: ratio(bar.vol, volume_ma20),
        volume_ma5_to_ma20_ratio: ratio(volume_ma5, volume_ma20),
        turnover_rate_ma5,
        turnover_to_ma5_ratio: ratio(bar.turnover_rate, turnover_rate_ma5),
        range_compression_20d: range_compression(high_20, low_20, close),
        range_compression_40d: range_compression(
            rolling_extreme(bars, idx, 40, PriceField::High, Extreme::Max),
            rolling_extreme(bars, idx, 40, PriceField::Low, Extreme::Min),
            close,
        ),
    }
}

fn compute_left_peak_row(bars: &[SourceBar], idx: usize) -> LeftPeakRow {
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

#[derive(Clone, Copy)]
enum PriceField {
    High,
    Low,
    Close,
    Vol,
    TurnoverRate,
}

#[derive(Clone, Copy)]
enum Extreme {
    Max,
    Min,
}

fn rolling_average(
    bars: &[SourceBar],
    idx: usize,
    window: usize,
    field: PriceField,
) -> Option<f64> {
    let values = rolling_values(bars, idx, window, field)?;
    Some(values.iter().sum::<f64>() / values.len() as f64)
}

fn rolling_extreme(
    bars: &[SourceBar],
    idx: usize,
    window: usize,
    field: PriceField,
    extreme: Extreme,
) -> Option<f64> {
    let values = rolling_values(bars, idx, window, field)?;
    match extreme {
        Extreme::Max => values.into_iter().reduce(f64::max),
        Extreme::Min => values.into_iter().reduce(f64::min),
    }
}

fn rolling_values(
    bars: &[SourceBar],
    idx: usize,
    window: usize,
    field: PriceField,
) -> Option<Vec<f64>> {
    if idx + 1 < window {
        return None;
    }
    bars[idx + 1 - window..=idx]
        .iter()
        .map(|bar| value(bar, field))
        .collect()
}

fn value(bar: &SourceBar, field: PriceField) -> Option<f64> {
    match field {
        PriceField::High => bar.high_qfq,
        PriceField::Low => bar.low_qfq,
        PriceField::Close => bar.close_qfq,
        PriceField::Vol => bar.vol,
        PriceField::TurnoverRate => bar.turnover_rate,
    }
}

fn ratio(numerator: Option<f64>, denominator: Option<f64>) -> Option<f64> {
    let denominator = denominator?;
    if denominator == 0.0 {
        return None;
    }
    Some(numerator? / denominator)
}

fn ratio_minus_one(numerator: Option<f64>, denominator: Option<f64>) -> Option<f64> {
    ratio(numerator, denominator).map(|value| value - 1.0)
}

fn position(close: Option<f64>, low: Option<f64>, high: Option<f64>) -> Option<f64> {
    let low = low?;
    let high = high?;
    let denominator = high - low;
    if denominator == 0.0 {
        return None;
    }
    Some((close? - low) / denominator)
}

fn range_compression(high: Option<f64>, low: Option<f64>, close: Option<f64>) -> Option<f64> {
    let close = close?;
    if close == 0.0 {
        return None;
    }
    Some((high? - low?) / close)
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

fn close_or_nan(close: Option<f64>) -> f64 {
    close.expect("MACD state only exists with close_qfq")
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::model::period_start_date;

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
    fn rolling_values_require_complete_window() {
        let bars = (1..=25)
            .map(|day| bar("000001.SZ", 2025, 1, day, day as f64))
            .collect::<Vec<_>>();
        assert_eq!(rolling_average(&bars, 23, 25, PriceField::Close), None);
        assert_eq!(
            rolling_average(&bars, 24, 25, PriceField::Close),
            Some(13.0)
        );
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

    #[test]
    fn computes_index_asof_rows_for_target_dates() {
        let bars = vec![
            bar("885001.TI", 2025, 1, 2, 10.0),
            bar("885001.TI", 2025, 1, 3, 11.0),
            bar("885001.TI", 2025, 1, 6, 12.0),
        ];
        let target_dates = vec![NaiveDate::from_ymd_opt(2025, 1, 6).unwrap()];

        let computed = compute_calc_rows(&[], &bars, &target_dates, 1);

        assert_eq!(computed.index_asof_rows.len(), 1);
        assert_eq!(computed.index_period_states.len(), 4);
        let daily_count = computed
            .index_period_states
            .iter()
            .filter(|row| row.period_type == "daily")
            .count();
        assert_eq!(daily_count, 3);
        let weekly_state = computed
            .index_period_states
            .iter()
            .find(|row| row.period_type == "weekly")
            .expect("weekly state exists");
        assert_eq!(weekly_state.ts_code, "885001.TI");
        assert_eq!(
            weekly_state.period_end_date,
            NaiveDate::from_ymd_opt(2025, 1, 3).unwrap()
        );
        assert_eq!(weekly_state.period_close_qfq, Some(11.0));
        let row = &computed.index_asof_rows[0];
        assert_eq!(row.ts_code, "885001.TI");
        assert_eq!(row.trade_date, target_dates[0]);
        assert_eq!(row.daily_period_count, Some(3));
        assert_eq!(row.weekly_period_count, Some(2));
        assert_eq!(row.monthly_period_count, Some(1));
    }

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

    fn bar(ts_code: &str, year: i32, month: u32, day: u32, close_qfq: f64) -> SourceBar {
        SourceBar {
            ts_code: ts_code.to_string(),
            trade_date: NaiveDate::from_ymd_opt(year, month, day).unwrap(),
            open_qfq: Some(close_qfq),
            high_qfq: Some(close_qfq + 1.0),
            low_qfq: Some(close_qfq - 1.0),
            close_qfq: Some(close_qfq),
            vol: Some(close_qfq),
            turnover_rate: Some(close_qfq),
        }
    }
}
