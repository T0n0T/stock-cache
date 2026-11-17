//! Daily point-in-time rolling factors used by the v3 calculation contract.

use anyhow::{ensure, Result};
use chrono::NaiveDate;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RollingBar {
    pub trade_date: NaiveDate,
    pub open: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub close: Option<f64>,
    pub volume: Option<f64>,
    pub turnover_rate: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct RollingValue {
    pub trade_date: NaiveDate,
    pub ma25_qfq: Option<f64>,
    pub ma144_qfq: Option<f64>,
    pub ma220_qfq: Option<f64>,
    pub high_20_qfq: Option<f64>,
    pub high_90_qfq: Option<f64>,
    pub low_90_qfq: Option<f64>,
    pub high_120_qfq: Option<f64>,
    pub low_120_qfq: Option<f64>,
    pub close_to_20d_max_close_pct: Option<f64>,
    pub position_90d: Option<f64>,
    pub position_120d: Option<f64>,
    pub volume_ma5: Option<f64>,
    pub volume_ma20: Option<f64>,
    pub volume_to_ma5_ratio: Option<f64>,
    pub volume_to_ma20_ratio: Option<f64>,
    pub volume_ma5_to_ma20_ratio: Option<f64>,
    pub turnover_rate_ma5: Option<f64>,
    pub turnover_to_ma5_ratio: Option<f64>,
    pub range_compression_20d: Option<f64>,
    pub range_compression_40d: Option<f64>,
}

pub fn calculate_rolling(bars: &[RollingBar]) -> Result<Vec<RollingValue>> {
    validate_dates(bars)?;
    Ok((0..bars.len()).map(|idx| compute_row(bars, idx)).collect())
}

fn compute_row(bars: &[RollingBar], idx: usize) -> RollingValue {
    let bar = bars[idx];
    let close = bar.close;
    let high_20 = rolling_extreme(bars, idx, 20, Field::High, Extreme::Max);
    let low_20 = rolling_extreme(bars, idx, 20, Field::Low, Extreme::Min);
    let high_90 = rolling_extreme(bars, idx, 90, Field::High, Extreme::Max);
    let low_90 = rolling_extreme(bars, idx, 90, Field::Low, Extreme::Min);
    let high_120 = rolling_extreme(bars, idx, 120, Field::High, Extreme::Max);
    let low_120 = rolling_extreme(bars, idx, 120, Field::Low, Extreme::Min);
    let max_close_20 = rolling_extreme(bars, idx, 20, Field::Close, Extreme::Max);
    let volume_ma5 = rolling_average(bars, idx, 5, Field::Volume);
    let volume_ma20 = rolling_average(bars, idx, 20, Field::Volume);
    let turnover_rate_ma5 = rolling_average(bars, idx, 5, Field::TurnoverRate);
    RollingValue {
        trade_date: bar.trade_date,
        ma25_qfq: rolling_average(bars, idx, 25, Field::Close),
        ma144_qfq: rolling_average(bars, idx, 144, Field::Close),
        ma220_qfq: rolling_average(bars, idx, 220, Field::Close),
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
        volume_to_ma5_ratio: ratio(bar.volume, volume_ma5),
        volume_to_ma20_ratio: ratio(bar.volume, volume_ma20),
        volume_ma5_to_ma20_ratio: ratio(volume_ma5, volume_ma20),
        turnover_rate_ma5,
        turnover_to_ma5_ratio: ratio(bar.turnover_rate, turnover_rate_ma5),
        range_compression_20d: range_compression(high_20, low_20, close),
        range_compression_40d: range_compression(
            rolling_extreme(bars, idx, 40, Field::High, Extreme::Max),
            rolling_extreme(bars, idx, 40, Field::Low, Extreme::Min),
            close,
        ),
    }
}

#[derive(Clone, Copy)]
enum Field {
    High,
    Low,
    Close,
    Volume,
    TurnoverRate,
}
#[derive(Clone, Copy)]
enum Extreme {
    Max,
    Min,
}

fn rolling_average(bars: &[RollingBar], idx: usize, window: usize, field: Field) -> Option<f64> {
    let values = rolling_values(bars, idx, window, field)?;
    Some(values.iter().sum::<f64>() / values.len() as f64)
}

fn rolling_extreme(
    bars: &[RollingBar],
    idx: usize,
    window: usize,
    field: Field,
    extreme: Extreme,
) -> Option<f64> {
    let values = rolling_values(bars, idx, window, field)?;
    match extreme {
        Extreme::Max => values.into_iter().reduce(f64::max),
        Extreme::Min => values.into_iter().reduce(f64::min),
    }
}

fn rolling_values(
    bars: &[RollingBar],
    idx: usize,
    window: usize,
    field: Field,
) -> Option<Vec<f64>> {
    if idx + 1 < window {
        return None;
    }
    bars[idx + 1 - window..=idx]
        .iter()
        .map(|bar| value(*bar, field))
        .collect()
}

fn value(bar: RollingBar, field: Field) -> Option<f64> {
    match field {
        Field::High => bar.high,
        Field::Low => bar.low,
        Field::Close => bar.close,
        Field::Volume => bar.volume,
        Field::TurnoverRate => bar.turnover_rate,
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
    let (close, low, high) = (close?, low?, high?);
    if high == low {
        return None;
    }
    Some((close - low) / (high - low))
}
fn range_compression(high: Option<f64>, low: Option<f64>, close: Option<f64>) -> Option<f64> {
    let (high, low, close) = (high?, low?, close?);
    if close == 0.0 {
        return None;
    }
    Some((high - low) / close)
}
fn validate_dates(bars: &[RollingBar]) -> Result<()> {
    for pair in bars.windows(2) {
        ensure!(
            pair[0].trade_date < pair[1].trade_date,
            "rolling bars must be strictly increasing"
        );
    }
    for bar in bars {
        for value in [
            bar.open,
            bar.high,
            bar.low,
            bar.close,
            bar.volume,
            bar.turnover_rate,
        ]
        .into_iter()
        .flatten()
        {
            ensure!(value.is_finite(), "rolling input contains non-finite value");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_require_complete_values() {
        let bars = (1..=25)
            .map(|day| RollingBar {
                trade_date: NaiveDate::from_ymd_opt(2025, 1, day).unwrap(),
                open: Some(day as f64),
                high: Some(day as f64 + 1.0),
                low: Some(day as f64 - 1.0),
                close: Some(day as f64),
                volume: Some(day as f64),
                turnover_rate: Some(day as f64),
            })
            .collect::<Vec<_>>();
        let rows = calculate_rolling(&bars).unwrap();
        assert!(rows[23].ma25_qfq.is_none());
        assert_eq!(rows[24].ma25_qfq, Some(13.0));
        assert_eq!(rows[24].high_20_qfq, Some(26.0));
    }
}
