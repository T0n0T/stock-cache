use std::collections::BTreeSet;

use serde::Deserialize;

const SOURCE_COMMIT: &str = "9ab62854f6bfab8515115b942baf4deb0f06185c";
const FIXTURES: [(&str, &str); 2] = [
    (
        "anonymized real daily bars",
        include_str!("fixtures/czsc/anonymized_real_daily.json"),
    ),
    (
        "handcrafted edge cases",
        include_str!("fixtures/czsc/handcrafted_daily.json"),
    ),
];

#[derive(Debug, Deserialize)]
struct Fixture {
    schema_version: u32,
    metadata: Metadata,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Metadata {
    source_czsc_commit: String,
    source_czsc_version: String,
    source_license: String,
    min_bi_len: usize,
    max_bi_num: usize,
    price_mode: String,
}

#[derive(Debug, Deserialize)]
struct Case {
    id: String,
    origin: String,
    covers: Vec<String>,
    bars: Vec<Bar>,
    expected: Vec<ExpectedDay>,
}

#[derive(Debug, Deserialize)]
struct Bar {
    trade_date: String,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
}

#[derive(Debug, Deserialize)]
struct ExpectedDay {
    trade_date: String,
    source_bar_valid: bool,
    bars_ubi_count: usize,
    bi_list_count: usize,
    bi_list: Vec<ExpectedBi>,
    finished_bis: Vec<ExpectedBi>,
    zs_seq: Vec<ExpectedZs>,
    events: Vec<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct ExpectedBi {
    direction: String,
    start_date: String,
    end_date: String,
    high: f64,
    low: f64,
}

#[derive(Debug, Deserialize, PartialEq)]
struct ExpectedZs {
    start_date: String,
    end_date: String,
    bi_count: usize,
    zg: f64,
    zd: f64,
    zz: f64,
    is_valid: bool,
}

fn load_fixtures() -> Vec<(&'static str, Fixture)> {
    FIXTURES
        .iter()
        .map(|(label, contents)| {
            let fixture = serde_json::from_str(contents)
                .unwrap_or_else(|error| panic!("failed to parse {label}: {error}"));
            (*label, fixture)
        })
        .collect()
}

#[test]
fn golden_fixtures_are_self_contained_and_point_in_time() {
    for (label, fixture) in load_fixtures() {
        assert_eq!(fixture.schema_version, 1, "{label}");
        assert_eq!(
            fixture.metadata.source_czsc_commit, SOURCE_COMMIT,
            "{label}"
        );
        assert_eq!(
            fixture.metadata.source_czsc_version, "1.0.0-rc.8",
            "{label}"
        );
        assert_eq!(fixture.metadata.source_license, "Apache-2.0", "{label}");
        assert_eq!(fixture.metadata.min_bi_len, 6, "{label}");
        assert_eq!(fixture.metadata.max_bi_num, 50, "{label}");
        assert_eq!(fixture.metadata.price_mode, "qfq", "{label}");
        assert!(!fixture.cases.is_empty(), "{label}");

        for case in fixture.cases {
            assert!(!case.id.is_empty(), "{label}");
            assert!(!case.origin.is_empty(), "{} in {label}", case.id);
            assert_eq!(
                case.bars.len(),
                case.expected.len(),
                "{} in {label}",
                case.id
            );
            assert!(!case.bars.is_empty(), "{} in {label}", case.id);

            let mut previous_date = None;
            let mut previous_expected: Option<&ExpectedDay> = None;
            for (bar, expected) in case.bars.iter().zip(&case.expected) {
                assert_eq!(
                    bar.trade_date, expected.trade_date,
                    "{} in {label}",
                    case.id
                );
                if let Some(previous_date) = previous_date {
                    assert!(
                        bar.trade_date.as_str() > previous_date,
                        "{} in {label}",
                        case.id
                    );
                }
                previous_date = Some(bar.trade_date.as_str());

                let valid = bar.open.is_finite()
                    && bar.high.is_finite()
                    && bar.low.is_finite()
                    && bar.close.is_finite()
                    && bar.open > 0.0
                    && bar.high > 0.0
                    && bar.low > 0.0
                    && bar.close > 0.0
                    && bar.high >= bar.open.max(bar.close)
                    && bar.low <= bar.open.min(bar.close)
                    && bar.high >= bar.low;
                assert_eq!(
                    valid, expected.source_bar_valid,
                    "{} on {}",
                    case.id, bar.trade_date
                );
                assert!(expected.bars_ubi_count <= case.bars.len());
                assert!(
                    expected.bi_list_count <= 50,
                    "{} on {}",
                    case.id,
                    bar.trade_date
                );
                assert_eq!(expected.bi_list_count, expected.bi_list.len());
                assert!(expected.finished_bis.len() <= expected.bi_list_count);

                for bi in &expected.bi_list {
                    assert!(matches!(bi.direction.as_str(), "up" | "down"));
                    assert!(bi.start_date <= bi.end_date);
                    assert!(bi.high >= bi.low);
                }
                for bi in &expected.finished_bis {
                    assert!(matches!(bi.direction.as_str(), "up" | "down"));
                    assert!(bi.start_date <= bi.end_date);
                    assert!(bi.high >= bi.low);
                }
                for zs in &expected.zs_seq {
                    assert!(zs.start_date <= zs.end_date);
                    assert!(zs.bi_count > 0);
                    assert!((zs.zz - (zs.zg + zs.zd) / 2.0).abs() <= 1e-9);
                    if zs.is_valid {
                        assert!(zs.zg >= zs.zd);
                    }
                }

                let allowed_events = [
                    "last_bi_extension",
                    "last_bi_break",
                    "last_bi_removal",
                    "finished_tail_included",
                    "finished_tail_excluded",
                ];
                assert!(expected
                    .events
                    .iter()
                    .all(|event| allowed_events.contains(&event.as_str())));
                if !expected.source_bar_valid {
                    let previous =
                        previous_expected.expect("invalid first bar has no state to carry");
                    assert_eq!(expected.bars_ubi_count, previous.bars_ubi_count);
                    assert_eq!(expected.bi_list_count, previous.bi_list_count);
                    assert_eq!(expected.bi_list, previous.bi_list);
                    assert_eq!(expected.finished_bis, previous.finished_bis);
                    assert_eq!(expected.zs_seq, previous.zs_seq);
                    assert!(expected.events.is_empty());
                }
                previous_expected = Some(expected);
            }
        }
    }
}

#[test]
fn fixtures_cover_real_data_and_all_batch_a_edges() {
    let fixtures = load_fixtures();
    let origins = fixtures
        .iter()
        .flat_map(|(_, fixture)| fixture.cases.iter().map(|case| case.origin.as_str()))
        .collect::<BTreeSet<_>>();
    assert!(origins.contains("anonymized_real_market"));
    assert!(origins.contains("handcrafted"));

    let coverage = fixtures
        .iter()
        .flat_map(|(_, fixture)| fixture.cases.iter())
        .flat_map(|case| case.covers.iter().map(String::as_str))
        .collect::<BTreeSet<_>>();
    let required = [
        "monotone_up",
        "monotone_down",
        "wide_oscillation",
        "bar_inclusion",
        "gap",
        "resume_after_suspension",
        "tied_high",
        "doji",
        "invalid_ohlc",
        "last_bi_extension",
        "last_bi_break",
        "last_bi_removal",
    ];
    for behavior in required {
        assert!(coverage.contains(behavior), "missing coverage: {behavior}");
    }

    for (_, fixture) in &fixtures {
        for case in &fixture.cases {
            if case.covers.iter().any(|tag| tag == "monotone_up") {
                assert!(case
                    .bars
                    .windows(2)
                    .all(|bars| bars[1].close > bars[0].close));
            }
            if case.covers.iter().any(|tag| tag == "monotone_down") {
                assert!(case
                    .bars
                    .windows(2)
                    .all(|bars| bars[1].close < bars[0].close));
            }
            if case.covers.iter().any(|tag| tag == "bar_inclusion") {
                assert!(case.bars.windows(2).any(|bars| {
                    (bars[0].high >= bars[1].high && bars[0].low <= bars[1].low)
                        || (bars[1].high >= bars[0].high && bars[1].low <= bars[0].low)
                }));
            }
            if case.covers.iter().any(|tag| tag == "gap") {
                assert!(case
                    .bars
                    .windows(2)
                    .any(|bars| bars[1].low > bars[0].high || bars[1].high < bars[0].low));
            }
            if case.covers.iter().any(|tag| tag == "wide_oscillation") {
                let highest = case
                    .bars
                    .iter()
                    .map(|bar| bar.high)
                    .fold(f64::NEG_INFINITY, f64::max);
                let lowest = case
                    .bars
                    .iter()
                    .map(|bar| bar.low)
                    .fold(f64::INFINITY, f64::min);
                assert!(highest / lowest >= 2.0);
                assert!(case.expected.iter().any(|day| day.bi_list_count >= 3));
            }
            if case
                .covers
                .iter()
                .any(|tag| tag == "resume_after_suspension")
            {
                assert!(case.bars.windows(2).any(|bars| {
                    let left = chrono::NaiveDate::parse_from_str(&bars[0].trade_date, "%Y-%m-%d")
                        .expect("valid fixture date");
                    let right = chrono::NaiveDate::parse_from_str(&bars[1].trade_date, "%Y-%m-%d")
                        .expect("valid fixture date");
                    (right - left).num_days() >= 10
                }));
            }
            if case.covers.iter().any(|tag| tag == "tied_high") {
                assert!(case
                    .bars
                    .windows(2)
                    .any(|bars| bars[0].high == bars[1].high));
            }
            if case.covers.iter().any(|tag| tag == "doji") {
                assert!(case.bars.iter().any(|bar| bar.open == bar.close));
            }
            if case.covers.iter().any(|tag| tag == "invalid_ohlc") {
                assert!(case.expected.iter().any(|day| !day.source_bar_valid));
            }
        }
    }

    let observed_events = fixtures
        .iter()
        .flat_map(|(_, fixture)| fixture.cases.iter())
        .flat_map(|case| &case.expected)
        .flat_map(|day| day.events.iter().map(String::as_str))
        .collect::<BTreeSet<_>>();
    for event in [
        "last_bi_extension",
        "last_bi_break",
        "last_bi_removal",
        "finished_tail_included",
        "finished_tail_excluded",
    ] {
        assert!(
            observed_events.contains(event),
            "event never occurs: {event}"
        );
    }

    for (_, fixture) in &fixtures {
        for case in &fixture.cases {
            let mut removed = Vec::new();
            for (previous, current) in case.expected.iter().zip(case.expected.iter().skip(1)) {
                if current
                    .events
                    .iter()
                    .any(|event| event == "last_bi_removal")
                {
                    assert!(current.bi_list.len() < previous.bi_list.len());
                    assert!(current.events.iter().any(|event| event == "last_bi_break"));
                    removed.push(previous.bi_list.last().expect("removed BI exists"));
                }
                if current
                    .events
                    .iter()
                    .any(|event| event == "last_bi_extension")
                {
                    let extended = current.bi_list.last().expect("extended BI exists");
                    assert!(removed.iter().any(|old| {
                        old.direction == extended.direction
                            && old.start_date == extended.start_date
                            && ((extended.direction == "up"
                                && (extended.end_date > old.end_date || extended.high > old.high))
                                || (extended.direction == "down"
                                    && (extended.end_date > old.end_date
                                        || extended.low < old.low)))
                    }));
                }
            }
        }
    }
}
