use chrono::NaiveDate;
use serde::Deserialize;

use super::{AnalyzerSnapshot, CzscAnalyzer, CzscBar, CzscConfig, CzscSourceBar};

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    bars: Vec<FixtureBar>,
    expected: Vec<ExpectedDay>,
}

#[derive(Deserialize)]
struct FixtureBar {
    trade_date: String,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
}

#[derive(Deserialize)]
struct ExpectedDay {
    bi_list: Vec<ExpectedBi>,
    finished_bis: Vec<ExpectedBi>,
    snapshot: ExpectedSnapshot,
}

#[derive(Deserialize)]
struct ExpectedBi {
    direction: String,
    start_date: String,
    end_date: String,
    high: f64,
    low: f64,
}

#[derive(Deserialize)]
struct ExpectedSnapshot {
    source_bar_valid: bool,
    status: String,
    available_finished_bi_count: usize,
    recent_bi_count: usize,
    last_bi_direction: Option<String>,
    last_bi_start_date: Option<String>,
    last_bi_end_date: Option<String>,
    last_bi_high: Option<f64>,
    last_bi_low: Option<f64>,
    last_bi_mid: Option<f64>,
    latest_zs_start_date: Option<String>,
    latest_zs_end_date: Option<String>,
    latest_zs_bi_count: Option<usize>,
    latest_zs_zg: Option<f64>,
    latest_zs_zd: Option<f64>,
    latest_zs_zz: Option<f64>,
    recent_peak_date: Option<String>,
    recent_peak_high: Option<f64>,
    post_peak_first_bear_date: Option<String>,
    post_peak_first_bear_open: Option<f64>,
    post_peak_first_bear_high: Option<f64>,
    post_peak_first_bear_low: Option<f64>,
    post_peak_first_bear_close: Option<f64>,
    post_peak_first_bear_missing: Option<bool>,
}

#[test]
fn analyzer_matches_daily_bi_golden_baseline() {
    for contents in [
        include_str!("../../../tests/fixtures/czsc/anonymized_real_daily.json"),
        include_str!("../../../tests/fixtures/czsc/handcrafted_daily.json"),
    ] {
        let fixture: Fixture = serde_json::from_str(contents).expect("valid fixture");
        for case in fixture.cases {
            let mut analyzer = CzscAnalyzer::new(CzscConfig {
                min_bi_len: 6,
                max_bi_num: 50,
            });
            for (bar, expected) in case.bars.into_iter().zip(case.expected) {
                let trade_date = parse_date(&bar.trade_date);
                if CzscBar::is_valid_ohlc(bar.open, bar.high, bar.low, bar.close) {
                    analyzer
                        .update(CzscBar {
                            trade_date,
                            open: bar.open,
                            high: bar.high,
                            low: bar.low,
                            close: bar.close,
                        })
                        .expect("ordered fixture bar");
                }

                assert_bis(&analyzer.bi_list(), &expected.bi_list);
                assert_bis(&analyzer.finished_bis(), &expected.finished_bis);
            }
        }
    }
}

#[test]
fn analyzer_rejects_duplicate_and_out_of_order_dates() {
    let mut analyzer = CzscAnalyzer::new(CzscConfig {
        min_bi_len: 6,
        max_bi_num: 50,
    });
    let first = CzscBar {
        trade_date: parse_date("2024-01-02"),
        open: 10.0,
        high: 11.0,
        low: 9.0,
        close: 10.5,
    };
    analyzer.update(first).expect("first bar");
    assert!(analyzer.update(first).is_err());
    let earlier = CzscBar {
        trade_date: parse_date("2024-01-01"),
        ..first
    };
    assert!(analyzer.update(earlier).is_err());
}

#[test]
fn analyzer_caps_internal_bi_history() {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../../tests/fixtures/czsc/anonymized_real_daily.json"
    ))
    .expect("valid fixture");
    let case = fixture.cases.into_iter().next().expect("real case");
    let mut analyzer = CzscAnalyzer::new(CzscConfig {
        min_bi_len: 6,
        max_bi_num: 2,
    });
    for bar in case.bars {
        analyzer
            .update(CzscBar {
                trade_date: parse_date(&bar.trade_date),
                open: bar.open,
                high: bar.high,
                low: bar.low,
                close: bar.close,
            })
            .expect("ordered valid bar");
        assert!(analyzer.bi_list().len() <= 2);
    }
    assert_eq!(analyzer.bi_list().len(), 2);
}

#[test]
fn analyzer_matches_daily_structure_snapshot_golden_baseline() {
    for contents in [
        include_str!("../../../tests/fixtures/czsc/anonymized_real_daily.json"),
        include_str!("../../../tests/fixtures/czsc/handcrafted_daily.json"),
    ] {
        let fixture: Fixture = serde_json::from_str(contents).expect("valid fixture");
        for case in fixture.cases {
            let mut analyzer = CzscAnalyzer::new(CzscConfig {
                min_bi_len: 6,
                max_bi_num: 50,
            });
            for (bar, expected) in case.bars.into_iter().zip(case.expected) {
                let snapshot = analyzer
                    .process(CzscSourceBar {
                        trade_date: parse_date(&bar.trade_date),
                        open: Some(bar.open),
                        high: Some(bar.high),
                        low: Some(bar.low),
                        close: Some(bar.close),
                    })
                    .expect("ordered fixture source bar");
                assert_snapshot(&snapshot, &expected.snapshot);
            }
        }
    }
}

#[test]
fn golden_scenarios_exercise_rolling_anchor_and_unfinished_bear_search() {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../../tests/fixtures/czsc/handcrafted_daily.json"
    ))
    .expect("valid fixture");
    let case = fixture
        .cases
        .into_iter()
        .find(|case| case.bars.len() > 20)
        .expect("swing lifecycle case");
    assert!(case.expected.windows(2).any(|days| {
        days[0].snapshot.recent_peak_date.as_deref() == Some("2024-03-12")
            && days[1].snapshot.recent_peak_date.as_deref() == Some("2024-01-31")
    }));
    assert!(case.expected.iter().any(|day| {
        day.snapshot.post_peak_first_bear_date.is_some()
            && day.bi_list.len() > day.finished_bis.len()
    }));
}

fn assert_bis(actual: &[super::model::BiSummary], expected: &[ExpectedBi]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!(actual.direction.as_str(), expected.direction);
        assert_eq!(actual.start_date, parse_date(&expected.start_date));
        assert_eq!(actual.end_date, parse_date(&expected.end_date));
        assert!((actual.high - expected.high).abs() <= 1e-9);
        assert!((actual.low - expected.low).abs() <= 1e-9);
    }
}

fn parse_date(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").expect("fixture date")
}

fn assert_snapshot(actual: &AnalyzerSnapshot, expected: &ExpectedSnapshot) {
    assert_eq!(actual.source_bar_valid, expected.source_bar_valid);
    assert_eq!(actual.status, expected.status);
    assert_eq!(
        actual.available_finished_bi_count,
        expected.available_finished_bi_count
    );
    assert_eq!(actual.recent_bi_count, expected.recent_bi_count);
    assert_eq!(actual.last_bi_direction, expected.last_bi_direction);
    assert_optional_date(
        actual.last_bi_start_date,
        expected.last_bi_start_date.as_deref(),
    );
    assert_optional_date(
        actual.last_bi_end_date,
        expected.last_bi_end_date.as_deref(),
    );
    assert_optional_float(actual.last_bi_high, expected.last_bi_high);
    assert_optional_float(actual.last_bi_low, expected.last_bi_low);
    assert_optional_float(actual.last_bi_mid, expected.last_bi_mid);
    assert_optional_date(
        actual.latest_zs_start_date,
        expected.latest_zs_start_date.as_deref(),
    );
    assert_optional_date(
        actual.latest_zs_end_date,
        expected.latest_zs_end_date.as_deref(),
    );
    assert_eq!(actual.latest_zs_bi_count, expected.latest_zs_bi_count);
    assert_optional_float(actual.latest_zs_zg, expected.latest_zs_zg);
    assert_optional_float(actual.latest_zs_zd, expected.latest_zs_zd);
    assert_optional_float(actual.latest_zs_zz, expected.latest_zs_zz);
    assert_optional_date(
        actual.recent_peak_date,
        expected.recent_peak_date.as_deref(),
    );
    assert_optional_float(actual.recent_peak_high, expected.recent_peak_high);
    assert_optional_date(
        actual.post_peak_first_bear_date,
        expected.post_peak_first_bear_date.as_deref(),
    );
    assert_optional_float(
        actual.post_peak_first_bear_open,
        expected.post_peak_first_bear_open,
    );
    assert_optional_float(
        actual.post_peak_first_bear_high,
        expected.post_peak_first_bear_high,
    );
    assert_optional_float(
        actual.post_peak_first_bear_low,
        expected.post_peak_first_bear_low,
    );
    assert_optional_float(
        actual.post_peak_first_bear_close,
        expected.post_peak_first_bear_close,
    );
    assert_eq!(
        actual.post_peak_first_bear_missing,
        expected.post_peak_first_bear_missing
    );

    assert_eq!(
        actual.last_bi_direction.is_some(),
        [
            actual.last_bi_start_date.is_some(),
            actual.last_bi_end_date.is_some(),
            actual.last_bi_high.is_some(),
            actual.last_bi_low.is_some(),
            actual.last_bi_mid.is_some(),
        ]
        .into_iter()
        .all(|present| present)
    );
    if let (Some(high), Some(low), Some(mid)) =
        (actual.last_bi_high, actual.last_bi_low, actual.last_bi_mid)
    {
        assert!(high >= low);
        assert!((mid - (high + low) / 2.0).abs() <= 1e-9);
    }
    let zs_presence = [
        actual.latest_zs_start_date.is_some(),
        actual.latest_zs_end_date.is_some(),
        actual.latest_zs_bi_count.is_some(),
        actual.latest_zs_zg.is_some(),
        actual.latest_zs_zd.is_some(),
        actual.latest_zs_zz.is_some(),
    ];
    assert!(zs_presence.into_iter().all(|present| present) || zs_presence.into_iter().all(|p| !p));
    assert_eq!(
        actual.recent_peak_date.is_some(),
        actual.recent_peak_high.is_some()
    );
    let bear_presence = [
        actual.post_peak_first_bear_date.is_some(),
        actual.post_peak_first_bear_open.is_some(),
        actual.post_peak_first_bear_high.is_some(),
        actual.post_peak_first_bear_low.is_some(),
        actual.post_peak_first_bear_close.is_some(),
    ];
    assert!(
        bear_presence.into_iter().all(|present| present) || bear_presence.into_iter().all(|p| !p)
    );
    match actual.post_peak_first_bear_missing {
        None => assert!(actual.recent_peak_date.is_none()),
        Some(true) => assert!(actual.recent_peak_date.is_some() && !bear_presence[0]),
        Some(false) => assert!(actual.recent_peak_date.is_some() && bear_presence[0]),
    }
}

fn assert_optional_date(actual: Option<NaiveDate>, expected: Option<&str>) {
    assert_eq!(actual, expected.map(parse_date));
}

fn assert_optional_float(actual: Option<f64>, expected: Option<f64>) {
    match (actual, expected) {
        (Some(actual), Some(expected)) => assert!((actual - expected).abs() <= 1e-9),
        (None, None) => {}
        pair => panic!("optional float mismatch: {pair:?}"),
    }
}
