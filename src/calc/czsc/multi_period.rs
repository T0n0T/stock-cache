use anyhow::{ensure, Result};
use chrono::{Datelike, NaiveDate};
use serde::Serialize;

use crate::calc::czsc::{CzscAnalyzer, CzscConfig, CzscSourceBar};

const MIN_BI_LEN: usize = 6;
const MAX_BI_NUM: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct OhlcBar {
    pub trade_date: NaiveDate,
    pub open: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub close: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CzscSnapshot {
    pub trade_date: NaiveDate,
    pub period_complete: bool,
    pub source_bar_valid: bool,
    pub status: String,
    pub available_finished_bi_count: usize,
    pub recent_bi_count: usize,
    pub last_bi_direction: Option<String>,
    pub last_bi_start_date: Option<NaiveDate>,
    pub last_bi_end_date: Option<NaiveDate>,
    pub last_bi_high: Option<f64>,
    pub last_bi_low: Option<f64>,
    pub last_bi_mid: Option<f64>,
    pub latest_zs_start_date: Option<NaiveDate>,
    pub latest_zs_end_date: Option<NaiveDate>,
    pub latest_zs_bi_count: Option<usize>,
    pub latest_zs_zg: Option<f64>,
    pub latest_zs_zd: Option<f64>,
    pub latest_zs_zz: Option<f64>,
    pub recent_peak_date: Option<NaiveDate>,
    pub recent_peak_high: Option<f64>,
    pub post_peak_first_bear_date: Option<NaiveDate>,
    pub post_peak_first_bear_open: Option<f64>,
    pub post_peak_first_bear_high: Option<f64>,
    pub post_peak_first_bear_low: Option<f64>,
    pub post_peak_first_bear_close: Option<f64>,
    pub post_peak_first_bear_missing: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MultiPeriodCzscRow {
    pub trade_date: NaiveDate,
    pub daily: CzscSnapshot,
    pub weekly: CzscSnapshot,
    pub monthly: CzscSnapshot,
}

pub fn calculate_czsc(bars: &[OhlcBar]) -> Result<Vec<CzscSnapshot>> {
    validate_dates(bars)?;
    let mut analyzer = CzscAnalyzer::new(CzscConfig {
        min_bi_len: MIN_BI_LEN,
        max_bi_num: MAX_BI_NUM,
    });
    bars.iter()
        .map(|bar| process_bar(&mut analyzer, *bar))
        .collect()
}

pub fn calculate_multi_period_czsc(
    bars: &[OhlcBar],
    trading_dates: &[NaiveDate],
) -> Result<Vec<MultiPeriodCzscRow>> {
    validate_dates(bars)?;
    validate_calendar(trading_dates)?;
    for bar in bars {
        ensure!(
            trading_dates.binary_search(&bar.trade_date).is_ok(),
            "bar date {} is absent from trading calendar",
            bar.trade_date
        );
    }
    let daily = calculate_czsc(bars)?;
    let mut rows = Vec::with_capacity(bars.len());
    for index in 0..bars.len() {
        let prefix = &bars[..=index];
        rows.push(MultiPeriodCzscRow {
            trade_date: bars[index].trade_date,
            daily: daily[index].clone(),
            weekly: period_snapshot(prefix, trading_dates, PeriodType::Weekly)?,
            monthly: period_snapshot(prefix, trading_dates, PeriodType::Monthly)?,
        });
    }
    Ok(rows)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PeriodType {
    Weekly,
    Monthly,
}

fn period_snapshot(
    bars: &[OhlcBar],
    trading_dates: &[NaiveDate],
    period_type: PeriodType,
) -> Result<CzscSnapshot> {
    let mut periods = Vec::new();
    for bar in bars {
        let key = period_key(period_type, bar.trade_date);
        if periods
            .last()
            .is_none_or(|period: &OhlcBar| period_key(period_type, period.trade_date) != key)
        {
            periods.push(OhlcBar {
                trade_date: bar.trade_date,
                open: bar.open,
                high: bar.high,
                low: bar.low,
                close: bar.close,
            });
        } else {
            let period = periods.last_mut().expect("period exists");
            period.trade_date = *trading_dates
                .iter()
                .rfind(|date| period_key(period_type, **date) == key)
                .expect("period calendar end exists");
            period.high = max_opt(period.high, bar.high);
            period.low = min_opt(period.low, bar.low);
            period.close = bar.close.or(period.close);
        }
    }
    let target = bars.last().expect("non-empty bars").trade_date;
    let period_complete = trading_dates
        .iter()
        .rev()
        .find(|date| period_key(period_type, **date) == period_key(period_type, target))
        .is_some_and(|end| *end == target);
    let mut analyzer = CzscAnalyzer::new(CzscConfig {
        min_bi_len: MIN_BI_LEN,
        max_bi_num: MAX_BI_NUM,
    });
    let mut snapshot = None;
    for period in periods {
        snapshot = Some(process_bar(&mut analyzer, period)?);
    }
    let mut snapshot = snapshot.unwrap_or_else(|| empty_snapshot(target));
    snapshot.trade_date = target;
    snapshot.period_complete = period_complete;
    Ok(snapshot)
}

fn process_bar(analyzer: &mut CzscAnalyzer, bar: OhlcBar) -> Result<CzscSnapshot> {
    let snapshot = analyzer.process(CzscSourceBar {
        trade_date: bar.trade_date,
        open: bar.open,
        high: bar.high,
        low: bar.low,
        close: bar.close,
    })?;
    Ok(CzscSnapshot {
        trade_date: bar.trade_date,
        period_complete: true,
        source_bar_valid: snapshot.source_bar_valid,
        status: snapshot.status,
        available_finished_bi_count: snapshot.available_finished_bi_count,
        recent_bi_count: snapshot.recent_bi_count,
        last_bi_direction: snapshot.last_bi_direction,
        last_bi_start_date: snapshot.last_bi_start_date,
        last_bi_end_date: snapshot.last_bi_end_date,
        last_bi_high: snapshot.last_bi_high,
        last_bi_low: snapshot.last_bi_low,
        last_bi_mid: snapshot.last_bi_mid,
        latest_zs_start_date: snapshot.latest_zs_start_date,
        latest_zs_end_date: snapshot.latest_zs_end_date,
        latest_zs_bi_count: snapshot.latest_zs_bi_count,
        latest_zs_zg: snapshot.latest_zs_zg,
        latest_zs_zd: snapshot.latest_zs_zd,
        latest_zs_zz: snapshot.latest_zs_zz,
        recent_peak_date: snapshot.recent_peak_date,
        recent_peak_high: snapshot.recent_peak_high,
        post_peak_first_bear_date: snapshot.post_peak_first_bear_date,
        post_peak_first_bear_open: snapshot.post_peak_first_bear_open,
        post_peak_first_bear_high: snapshot.post_peak_first_bear_high,
        post_peak_first_bear_low: snapshot.post_peak_first_bear_low,
        post_peak_first_bear_close: snapshot.post_peak_first_bear_close,
        post_peak_first_bear_missing: snapshot.post_peak_first_bear_missing,
    })
}

fn empty_snapshot(trade_date: NaiveDate) -> CzscSnapshot {
    CzscSnapshot {
        trade_date,
        period_complete: false,
        source_bar_valid: false,
        status: "no_source_bar".to_string(),
        available_finished_bi_count: 0,
        recent_bi_count: 0,
        last_bi_direction: None,
        last_bi_start_date: None,
        last_bi_end_date: None,
        last_bi_high: None,
        last_bi_low: None,
        last_bi_mid: None,
        latest_zs_start_date: None,
        latest_zs_end_date: None,
        latest_zs_bi_count: None,
        latest_zs_zg: None,
        latest_zs_zd: None,
        latest_zs_zz: None,
        recent_peak_date: None,
        recent_peak_high: None,
        post_peak_first_bear_date: None,
        post_peak_first_bear_open: None,
        post_peak_first_bear_high: None,
        post_peak_first_bear_low: None,
        post_peak_first_bear_close: None,
        post_peak_first_bear_missing: None,
    }
}

fn validate_dates(bars: &[OhlcBar]) -> Result<()> {
    for pair in bars.windows(2) {
        ensure!(
            pair[0].trade_date < pair[1].trade_date,
            "CZSC bars must be strictly increasing"
        );
    }
    Ok(())
}
fn validate_calendar(dates: &[NaiveDate]) -> Result<()> {
    for pair in dates.windows(2) {
        ensure!(pair[0] < pair[1], "calendar must be strictly increasing");
    }
    Ok(())
}
fn period_key(period_type: PeriodType, date: NaiveDate) -> (i32, u32) {
    match period_type {
        PeriodType::Weekly => {
            let iso = date.iso_week();
            (iso.year(), iso.week())
        }
        PeriodType::Monthly => (date.year(), date.month()),
    }
}
fn max_opt(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.max(y)),
        (Some(x), None) => Some(x),
        (None, Some(y)) => Some(y),
        _ => None,
    }
}
fn min_opt(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (Some(x), None) => Some(x),
        (None, Some(y)) => Some(y),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    #[test]
    fn invalid_ohlc_is_point_in_time_without_advancing_state() {
        let d1 = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
        let d2 = NaiveDate::from_ymd_opt(2025, 1, 3).unwrap();
        let rows = calculate_czsc(&[
            OhlcBar {
                trade_date: d1,
                open: Some(10.0),
                high: Some(11.0),
                low: Some(9.0),
                close: Some(10.5),
            },
            OhlcBar {
                trade_date: d2,
                open: Some(10.0),
                high: Some(8.0),
                low: Some(9.0),
                close: Some(9.5),
            },
        ])
        .unwrap();
        assert!(rows[0].source_bar_valid);
        assert!(!rows[1].source_bar_valid);
        assert_eq!(rows[1].status, "invalid_source_bar");
    }

    #[test]
    fn weekly_snapshot_keeps_asof_date_and_completion_flag() {
        let bars = [
            OhlcBar {
                trade_date: NaiveDate::from_ymd_opt(2025, 1, 6).unwrap(),
                open: Some(10.0),
                high: Some(11.0),
                low: Some(9.0),
                close: Some(10.0),
            },
            OhlcBar {
                trade_date: NaiveDate::from_ymd_opt(2025, 1, 7).unwrap(),
                open: Some(10.0),
                high: Some(12.0),
                low: Some(9.0),
                close: Some(11.0),
            },
        ];
        let calendar = (6..=10)
            .map(|day| NaiveDate::from_ymd_opt(2025, 1, day).unwrap())
            .collect::<Vec<_>>();
        let rows = calculate_multi_period_czsc(&bars, &calendar).unwrap();
        assert_eq!(rows[0].weekly.trade_date, bars[0].trade_date);
        assert!(!rows[0].weekly.period_complete);
        assert_eq!(rows[1].weekly.trade_date, bars[1].trade_date);
        assert!(!rows[1].weekly.period_complete);
    }

    #[derive(Debug, Deserialize)]
    struct GoldenFixture {
        cases: Vec<GoldenCase>,
    }

    #[derive(Debug, Deserialize)]
    struct GoldenCase {
        bars: Vec<GoldenBar>,
        expected: Vec<GoldenExpected>,
    }

    #[derive(Debug, Deserialize)]
    struct GoldenBar {
        trade_date: String,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    }

    #[derive(Debug, Deserialize)]
    struct GoldenExpected {
        source_bar_valid: bool,
    }

    #[test]
    fn wrapper_preserves_golden_source_bar_validity() {
        let fixture: GoldenFixture = serde_json::from_str(include_str!(
            "../../../tests/fixtures/czsc/handcrafted_daily.json"
        ))
        .unwrap();
        for case in fixture.cases {
            let bars = case
                .bars
                .iter()
                .map(|bar| OhlcBar {
                    trade_date: NaiveDate::parse_from_str(&bar.trade_date, "%Y-%m-%d").unwrap(),
                    open: Some(bar.open),
                    high: Some(bar.high),
                    low: Some(bar.low),
                    close: Some(bar.close),
                })
                .collect::<Vec<_>>();
            let rows = calculate_czsc(&bars).unwrap();
            assert_eq!(rows.len(), case.expected.len());
            for (row, expected) in rows.iter().zip(case.expected) {
                assert_eq!(row.source_bar_valid, expected.source_bar_valid);
            }
        }
    }
}
