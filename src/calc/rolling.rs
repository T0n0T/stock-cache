use super::model::{RollingRow, SourceBar};

pub(super) fn compute_rolling_row(bars: &[SourceBar], idx: usize) -> RollingRow {
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

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
