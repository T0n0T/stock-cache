use std::collections::{HashMap, VecDeque};

use anyhow::{ensure, Result};
use chrono::{Datelike, NaiveDate};
use serde::Serialize;

const SMOOTHING: f64 = 1.0 / 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct OhlcvBar {
    pub trade_date: NaiveDate,
    pub open: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub close: Option<f64>,
    pub volume: Option<f64>,
    pub amount: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct KdjValue {
    pub k: f64,
    pub d: f64,
    pub j: f64,
    pub period_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct KdjAsofRow {
    pub trade_date: NaiveDate,
    pub daily: Option<KdjValue>,
    pub weekly: Option<KdjValue>,
    pub monthly: Option<KdjValue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum PeriodType {
    Weekly,
    Monthly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum PeriodKey {
    Weekly { year: i32, week: u32 },
    Monthly { year: i32, month: u32 },
}

#[derive(Debug, Clone, Copy)]
struct PeriodBar {
    key: PeriodKey,
    high: Option<f64>,
    low: Option<f64>,
    close: Option<f64>,
    volume: Option<f64>,
    amount: Option<f64>,
}

pub fn calculate_kdj(bars: &[OhlcvBar], window: usize) -> Result<Vec<Option<KdjValue>>> {
    ensure!(window > 0, "KDJ window must be positive");
    validate_dates(bars)?;
    let mut values = Vec::with_capacity(bars.len());
    let mut previous: Option<KdjValue> = None;
    let mut extrema: Vec<Option<(f64, f64)>> = Vec::with_capacity(bars.len());
    for bar in bars {
        let (Some(high), Some(low), Some(close)) = (bar.high, bar.low, bar.close) else {
            extrema.push(None);
            values.push(None);
            continue;
        };
        extrema.push(Some((high, low)));
        let start = extrema.len().saturating_sub(window);
        let window_values = &extrema[start..];
        if window_values.len() < window || window_values.iter().any(Option::is_none) {
            values.push(None);
            continue;
        }
        let highest = window_values
            .iter()
            .flatten()
            .map(|value| value.0)
            .fold(f64::NEG_INFINITY, f64::max);
        let lowest = window_values
            .iter()
            .flatten()
            .map(|value| value.1)
            .fold(f64::INFINITY, f64::min);
        let rsv = if (highest - lowest).abs() < f64::EPSILON {
            50.0
        } else {
            (close - lowest) / (highest - lowest) * 100.0
        };
        let (k, d) = previous
            .map(|value| {
                let k = (1.0 - SMOOTHING) * value.k + SMOOTHING * rsv;
                let d = (1.0 - SMOOTHING) * value.d + SMOOTHING * k;
                (k, d)
            })
            .unwrap_or((rsv, rsv));
        let value = KdjValue {
            k,
            d,
            j: 3.0 * k - 2.0 * d,
            period_count: previous.map_or(1, |value| value.period_count + 1),
        };
        previous = Some(value);
        values.push(Some(value));
    }
    Ok(values)
}

pub fn calculate_multi_period_kdj(
    bars: &[OhlcvBar],
    trading_dates: &[NaiveDate],
    window: usize,
) -> Result<Vec<KdjAsofRow>> {
    ensure!(window > 0, "KDJ window must be positive");
    validate_dates(bars)?;
    validate_calendar(trading_dates)?;
    for bar in bars {
        ensure!(
            trading_dates.binary_search(&bar.trade_date).is_ok(),
            "bar date {} is absent from trading calendar",
            bar.trade_date
        );
        validate_numbers(*bar)?;
    }

    let daily_values = calculate_kdj(bars, window)?;
    let mut period_ends = HashMap::new();
    for &date in trading_dates {
        period_ends.insert(
            (PeriodType::Weekly, period_key(PeriodType::Weekly, date)),
            date,
        );
        period_ends.insert(
            (PeriodType::Monthly, period_key(PeriodType::Monthly, date)),
            date,
        );
    }
    let mut weekly = PeriodKdjAccumulator::new(PeriodType::Weekly, window);
    let mut monthly = PeriodKdjAccumulator::new(PeriodType::Monthly, window);
    let mut rows = Vec::with_capacity(bars.len());
    for index in 0..bars.len() {
        let bar = bars[index];
        rows.push(KdjAsofRow {
            trade_date: bar.trade_date,
            daily: daily_values[index],
            weekly: weekly.observe(
                bar,
                period_ends
                    .get(&(
                        PeriodType::Weekly,
                        period_key(PeriodType::Weekly, bar.trade_date),
                    ))
                    .copied(),
            )?,
            monthly: monthly.observe(
                bar,
                period_ends
                    .get(&(
                        PeriodType::Monthly,
                        period_key(PeriodType::Monthly, bar.trade_date),
                    ))
                    .copied(),
            )?,
        });
    }
    Ok(rows)
}

struct PeriodKdjAccumulator {
    period_type: PeriodType,
    window: usize,
    active: Option<PeriodBar>,
    recent: VecDeque<Option<(f64, f64)>>,
    state: Option<KdjValue>,
}

impl PeriodKdjAccumulator {
    fn new(period_type: PeriodType, window: usize) -> Self {
        Self {
            period_type,
            window,
            active: None,
            recent: VecDeque::with_capacity(window),
            state: None,
        }
    }

    fn observe(
        &mut self,
        bar: OhlcvBar,
        period_end: Option<NaiveDate>,
    ) -> Result<Option<KdjValue>> {
        let key = period_key(self.period_type, bar.trade_date);
        if self.active.is_some_and(|active| active.key != key) {
            self.finish_active();
        }
        if let Some(active) = self.active.as_mut() {
            active.high = max_opt(active.high, bar.high);
            active.low = min_opt(active.low, bar.low);
            active.close = bar.close.or(active.close);
            active.volume = sum_opt(active.volume, bar.volume);
            active.amount = sum_opt(active.amount, bar.amount);
        } else {
            self.active = Some(PeriodBar {
                key,
                high: bar.high,
                low: bar.low,
                close: bar.close,
                volume: bar.volume,
                amount: bar.amount,
            });
        }
        let preview = self.preview_active();
        if period_end.is_some_and(|end| end == bar.trade_date) {
            self.finish_active();
        }
        Ok(preview)
    }

    fn finish_active(&mut self) {
        let Some(active) = self.active.take() else {
            return;
        };
        let candidate = (active.high, active.low, active.close);
        let value = self.calculate(candidate);
        let rolling = match candidate {
            (Some(high), Some(low), Some(_)) => Some((high, low)),
            _ => None,
        };
        self.push_recent(rolling);
        if let Some(value) = value {
            self.state = Some(value);
        }
    }

    fn preview_active(&self) -> Option<KdjValue> {
        let active = self.active?;
        self.calculate((active.high, active.low, active.close))
    }

    fn calculate(&self, candidate: (Option<f64>, Option<f64>, Option<f64>)) -> Option<KdjValue> {
        let (Some(high), Some(low), Some(close)) = candidate else {
            return None;
        };
        let mut values = self.recent.iter().copied().collect::<Vec<_>>();
        values.push(Some((high, low)));
        if values.len() < self.window {
            return None;
        }
        let start = values.len() - self.window;
        if values[start..].iter().any(Option::is_none) {
            return None;
        }
        let highest = values[start..]
            .iter()
            .flatten()
            .map(|value| value.0)
            .fold(f64::NEG_INFINITY, f64::max);
        let lowest = values[start..]
            .iter()
            .flatten()
            .map(|value| value.1)
            .fold(f64::INFINITY, f64::min);
        let rsv = if (highest - lowest).abs() < f64::EPSILON {
            50.0
        } else {
            (close - lowest) / (highest - lowest) * 100.0
        };
        let (k, d) = self
            .state
            .map(|value| {
                (
                    (1.0 - SMOOTHING) * value.k + SMOOTHING * rsv,
                    (1.0 - SMOOTHING) * value.d
                        + SMOOTHING * ((1.0 - SMOOTHING) * value.k + SMOOTHING * rsv),
                )
            })
            .unwrap_or((rsv, rsv));
        Some(KdjValue {
            k,
            d,
            j: 3.0 * k - 2.0 * d,
            period_count: self.state.map_or(1, |value| value.period_count + 1),
        })
    }

    fn push_recent(&mut self, value: Option<(f64, f64)>) {
        self.recent.push_back(value);
        while self.recent.len() > self.window {
            self.recent.pop_front();
        }
    }
}

fn validate_dates(bars: &[OhlcvBar]) -> Result<()> {
    for pair in bars.windows(2) {
        ensure!(
            pair[0].trade_date < pair[1].trade_date,
            "bars must be strictly increasing"
        );
    }
    for bar in bars {
        validate_numbers(*bar)?;
    }
    Ok(())
}

fn validate_numbers(bar: OhlcvBar) -> Result<()> {
    validate_numbers_generic(&bar)
}
fn validate_numbers_generic(bar: &OhlcvBar) -> Result<()> {
    for value in [
        bar.open, bar.high, bar.low, bar.close, bar.volume, bar.amount,
    ]
    .into_iter()
    .flatten()
    {
        ensure!(value.is_finite(), "KDJ input contains non-finite value");
    }
    Ok(())
}

fn validate_calendar(dates: &[NaiveDate]) -> Result<()> {
    for pair in dates.windows(2) {
        ensure!(pair[0] < pair[1], "calendar must be strictly increasing");
    }
    Ok(())
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

fn max_opt(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    match (left, right) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        _ => None,
    }
}
fn min_opt(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    match (left, right) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        _ => None,
    }
}
fn sum_opt(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    match (left, right) {
        (Some(a), Some(b)) => Some(a + b),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2025, 1, day).unwrap()
    }
    #[test]
    fn kdj_seed_and_smoothing_are_deterministic() {
        let bars = (1..=12)
            .map(|day| OhlcvBar {
                trade_date: d(day),
                open: Some(10.0),
                high: Some(10.0 + day as f64),
                low: Some(1.0),
                close: Some(5.0 + day as f64),
                volume: None,
                amount: None,
            })
            .collect::<Vec<_>>();
        let values = calculate_kdj(&bars, 9).unwrap();
        assert!(values[..8].iter().all(Option::is_none));
        let first = values[8].unwrap();
        assert_eq!(first.period_count, 1);
        assert!((first.k - first.d).abs() < 1e-12);
        assert!((first.j - (3.0 * first.k - 2.0 * first.d)).abs() < 1e-12);
        let second = values[9].unwrap();
        let rsv = (15.0 - 1.0) / (20.0 - 1.0) * 100.0;
        let expected_k = (2.0 / 3.0) * first.k + (1.0 / 3.0) * rsv;
        let expected_d = (2.0 / 3.0) * first.d + (1.0 / 3.0) * expected_k;
        assert!((second.k - expected_k).abs() < 1e-12);
        assert!((second.d - expected_d).abs() < 1e-12);
    }

    #[test]
    fn period_kdj_does_not_seed_before_window_is_complete() {
        let bars = (1..=3)
            .map(|day| OhlcvBar {
                trade_date: d(day),
                open: Some(10.0),
                high: Some(10.0 + day as f64),
                low: Some(1.0),
                close: Some(5.0 + day as f64),
                volume: None,
                amount: None,
            })
            .collect::<Vec<_>>();
        let calendar = bars.iter().map(|bar| bar.trade_date).collect::<Vec<_>>();
        let rows = calculate_multi_period_kdj(&bars, &calendar, 9).unwrap();
        assert!(rows.iter().all(|row| row.weekly.is_none()));
        assert!(rows.iter().all(|row| row.monthly.is_none()));
    }

    #[test]
    fn missing_ohlc_invalidates_period_window() {
        let bars = (1..=10)
            .map(|day| OhlcvBar {
                trade_date: d(day),
                open: Some(10.0),
                high: (day != 5).then_some(10.0 + day as f64),
                low: Some(1.0),
                close: Some(5.0 + day as f64),
                volume: None,
                amount: None,
            })
            .collect::<Vec<_>>();
        let values = calculate_kdj(&bars, 3).unwrap();
        assert!(values[2].is_some());
        assert!(values[3].is_some());
        assert!(values[4].is_none());
        assert!(values[5].is_none());
        assert!(values[6].is_none());
        assert!(values[7].is_some());
        assert!(values[8].is_some());
    }
}
