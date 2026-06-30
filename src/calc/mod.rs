use std::{
    collections::{BTreeMap, HashSet},
    time::Instant,
};

use anyhow::Result;
use chrono::NaiveDate;
use serde::Serialize;
use serde_json::json;
use tokio_postgres::NoTls;

use crate::app::{config::Config, write::DateWindow};

mod constants;
mod coverage;
mod index;
mod left_peak;
mod macd;
mod model;
mod rolling;
mod source;
mod stock;
mod writer;

use coverage::{load_existing_calc_coverage, load_existing_period_state_keys, PeriodStateTable};
use source::{
    load_distinct_index_source_trade_dates, load_distinct_source_trade_dates,
    load_index_source_row_count, load_index_source_rows, load_source_row_count, load_source_rows,
    resolve_calc_dates,
};
use stock::{compute_calc_rows, distinct_trade_dates};
use writer::write_calc_rows;

const AUTO_CHUNK_THRESHOLD_DAYS: usize = 60;
const AUTO_CHUNK_SIZE_DAYS: usize = 20;

#[derive(Debug, Serialize)]
pub struct CalcSummary {
    pub status: &'static str,
    pub mode: &'static str,
    pub source_row_count: usize,
    pub target_trade_dates: Vec<String>,
    pub calc_row_counts: BTreeMap<String, usize>,
    pub usable_ranges: BTreeMap<String, Vec<Option<String>>>,
    pub message: &'static str,
}

pub async fn run_calc(
    window: &DateWindow,
    config: &Config,
    max_concurrency: u32,
    force_recalc: bool,
    requested_chunk_size: Option<usize>,
) -> Result<CalcSummary> {
    let calc_started = Instant::now();
    eprintln!("[calc] connect postgres start");
    let (mut client, connection) = tokio_postgres::connect(&config.postgres_dsn, NoTls).await?;
    eprintln!(
        "[calc] connect postgres done elapsed={:.1}s",
        calc_started.elapsed().as_secs_f64()
    );
    let connection_task = tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("postgres connection error: {error}");
        }
    });

    eprintln!("[calc] resolve target dates start");
    let started = Instant::now();
    let target_dates = resolve_calc_dates(&client, window).await?;
    eprintln!(
        "[calc] resolve target dates done target_days={} elapsed={:.1}s",
        target_dates.len(),
        started.elapsed().as_secs_f64()
    );
    let result = if target_dates.is_empty() {
        Ok(empty_summary())
    } else {
        let max_target_date = *target_dates.last().expect("checked non-empty");
        eprintln!("[calc] inspect existing rows start");
        let started = Instant::now();
        let existing_coverage = load_existing_calc_coverage(&client, &target_dates).await?;
        let target_dates_to_compute = if force_recalc {
            target_dates.clone()
        } else {
            target_dates
                .iter()
                .copied()
                .filter(|date| !existing_coverage.is_target_trade_date_complete(*date))
                .collect::<Vec<_>>()
        };
        eprintln!(
            "[calc] inspect existing rows done target_days={} pending_days={} skipped_days={} force_recalc={} elapsed={:.1}s",
            target_dates.len(),
            target_dates_to_compute.len(),
            target_dates.len().saturating_sub(target_dates_to_compute.len()),
            force_recalc,
            started.elapsed().as_secs_f64()
        );
        if target_dates_to_compute.is_empty() {
            eprintln!("[calc] all target trade dates already complete; skip load/compute/write");
            let source_row_count = load_source_row_count(&client, max_target_date).await?;
            let index_source_row_count =
                load_index_source_row_count(&client, max_target_date).await?;
            let distinct_dates = load_distinct_source_trade_dates(&client, max_target_date).await?;
            let distinct_index_dates =
                load_distinct_index_source_trade_dates(&client, max_target_date).await?;
            Ok(summary(
                source_row_count + index_source_row_count,
                &target_dates,
                &distinct_dates,
                &distinct_index_dates,
                BTreeMap::from([
                    ("stock_period_indicator_state".to_string(), 0),
                    ("index_period_indicator_state".to_string(), 0),
                    ("stock_daily_asof_indicators".to_string(), 0),
                    ("stock_daily_rolling_factors".to_string(), 0),
                    ("stock_daily_left_peak".to_string(), 0),
                    ("index_daily_asof_indicators".to_string(), 0),
                ]),
            ))
        } else {
            eprintln!(
                "[calc] load source rows start max_trade_date={}",
                max_target_date.format("%Y%m%d")
            );
            let started = Instant::now();
            let source_rows = load_source_rows(&client, max_target_date).await?;
            let index_source_rows = load_index_source_rows(&client, max_target_date).await?;
            eprintln!(
                "[calc] load source rows done stock_rows={} index_rows={} elapsed={:.1}s",
                source_rows.len(),
                index_source_rows.len(),
                started.elapsed().as_secs_f64()
            );
            let distinct_dates = distinct_trade_dates(&source_rows);
            let distinct_index_dates = distinct_trade_dates(&index_source_rows);
            let chunk_size =
                resolve_chunk_size(target_dates_to_compute.len(), requested_chunk_size);
            let chunk_count = target_dates_to_compute.len().div_ceil(chunk_size);
            eprintln!(
                "[calc] chunk plan pending_days={} chunk_size={} chunks={} requested_chunk_size={}",
                target_dates_to_compute.len(),
                chunk_size,
                chunk_count,
                requested_chunk_size
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "auto".to_string())
            );
            let mut row_counts = empty_row_counts();
            let mut seen_stock_period_state_keys = HashSet::new();
            let mut seen_index_period_state_keys = HashSet::new();
            for (chunk_index, chunk_dates) in target_dates_to_compute.chunks(chunk_size).enumerate()
            {
                eprintln!(
                    "[calc] chunk {}/{} compute rows start date_window={}..{} max_concurrency={} target_days={}",
                    chunk_index + 1,
                    chunk_count,
                    chunk_dates.first().expect("chunk is non-empty").format("%Y%m%d"),
                    chunk_dates.last().expect("chunk is non-empty").format("%Y%m%d"),
                    max_concurrency.max(1),
                    chunk_dates.len()
                );
                let started = Instant::now();
                let computed = compute_calc_rows(
                    &source_rows,
                    &index_source_rows,
                    chunk_dates,
                    max_concurrency,
                );
                eprintln!(
                    "[calc] chunk {}/{} compute rows done stock_period_states={} index_period_states={} asof={} rolling={} left_peak={} index_asof={} elapsed={:.1}s",
                    chunk_index + 1,
                    chunk_count,
                    computed.period_states.len(),
                    computed.index_period_states.len(),
                    computed.asof_rows.len(),
                    computed.rolling_rows.len(),
                    computed.left_peak_rows.len(),
                    computed.index_asof_rows.len(),
                    started.elapsed().as_secs_f64()
                );
                eprintln!(
                    "[calc] chunk {}/{} write rows start",
                    chunk_index + 1,
                    chunk_count
                );
                let started = Instant::now();
                let existing_stock_period_state_keys = if force_recalc {
                    seen_stock_period_state_keys.clone()
                } else {
                    let mut keys = load_existing_period_state_keys(
                        &client,
                        PeriodStateTable::Stock,
                        &computed.period_states,
                    )
                    .await?;
                    keys.extend(seen_stock_period_state_keys.iter().cloned());
                    keys
                };
                let existing_index_period_state_keys = if force_recalc {
                    seen_index_period_state_keys.clone()
                } else {
                    let mut keys = load_existing_period_state_keys(
                        &client,
                        PeriodStateTable::Index,
                        &computed.index_period_states,
                    )
                    .await?;
                    keys.extend(seen_index_period_state_keys.iter().cloned());
                    keys
                };
                let chunk_row_counts = write_calc_rows(
                    &mut client,
                    &computed,
                    &existing_stock_period_state_keys,
                    &existing_index_period_state_keys,
                )
                .await?;
                remember_period_state_keys(
                    &mut seen_stock_period_state_keys,
                    &mut seen_index_period_state_keys,
                    &computed,
                );
                merge_counts(&mut row_counts, &chunk_row_counts);
                eprintln!(
                    "[calc] chunk {}/{} write rows done elapsed={:.1}s",
                    chunk_index + 1,
                    chunk_count,
                    started.elapsed().as_secs_f64()
                );
            }
            Ok(summary(
                source_rows.len() + index_source_rows.len(),
                &target_dates,
                &distinct_dates,
                &distinct_index_dates,
                row_counts,
            ))
        }
    };

    connection_task.abort();
    eprintln!(
        "[calc] done elapsed={:.1}s",
        calc_started.elapsed().as_secs_f64()
    );
    result
}

pub fn summary_json(summary: &CalcSummary) -> Result<String> {
    Ok(serde_json::to_string_pretty(&json!(summary))?)
}

fn summary(
    source_row_count: usize,
    target_dates: &[NaiveDate],
    distinct_dates: &[NaiveDate],
    distinct_index_dates: &[NaiveDate],
    calc_row_counts: BTreeMap<String, usize>,
) -> CalcSummary {
    let target_start = target_dates.first().copied();
    let target_end = target_dates.last().copied();
    let mut usable_ranges = BTreeMap::new();
    usable_ranges.insert(
        "stock_daily_asof_indicators".to_string(),
        date_range(target_start, target_end),
    );
    usable_ranges.insert(
        "index_daily_asof_indicators".to_string(),
        date_range(usable_start(distinct_index_dates, 1), target_end),
    );
    usable_ranges.insert(
        "stock_daily_rolling_factors".to_string(),
        date_range(usable_start(distinct_dates, 220), target_end),
    );
    usable_ranges.insert(
        "stock_daily_left_peak".to_string(),
        date_range(usable_start(distinct_dates, 360), target_end),
    );
    CalcSummary {
        status: "success",
        mode: "calc",
        source_row_count,
        target_trade_dates: target_dates.iter().map(ToString::to_string).collect(),
        calc_row_counts,
        usable_ranges,
        message: "local calculation completed",
    }
}

fn empty_summary() -> CalcSummary {
    summary(0, &[], &[], &[], BTreeMap::new())
}

fn resolve_chunk_size(pending_days: usize, requested_chunk_size: Option<usize>) -> usize {
    if let Some(chunk_size) = requested_chunk_size {
        return chunk_size.max(1);
    }
    if pending_days > AUTO_CHUNK_THRESHOLD_DAYS {
        AUTO_CHUNK_SIZE_DAYS
    } else {
        pending_days.max(1)
    }
}

fn empty_row_counts() -> BTreeMap<String, usize> {
    BTreeMap::from([
        ("stock_period_indicator_state".to_string(), 0),
        ("index_period_indicator_state".to_string(), 0),
        ("stock_daily_asof_indicators".to_string(), 0),
        ("index_daily_asof_indicators".to_string(), 0),
        ("stock_daily_rolling_factors".to_string(), 0),
        ("stock_daily_left_peak".to_string(), 0),
    ])
}

fn merge_counts(target: &mut BTreeMap<String, usize>, source: &BTreeMap<String, usize>) {
    for (table, count) in source {
        *target.entry(table.clone()).or_insert(0) += count;
    }
}

fn remember_period_state_keys(
    stock_keys: &mut HashSet<(String, String, NaiveDate)>,
    index_keys: &mut HashSet<(String, String, NaiveDate)>,
    computed: &model::ComputedRows,
) {
    stock_keys.extend(computed.period_states.iter().map(|row| {
        (
            row.ts_code.clone(),
            row.period_type.clone(),
            row.period_end_date,
        )
    }));
    index_keys.extend(computed.index_period_states.iter().map(|row| {
        (
            row.ts_code.clone(),
            row.period_type.clone(),
            row.period_end_date,
        )
    }));
}

fn usable_start(distinct_dates: &[NaiveDate], window: usize) -> Option<NaiveDate> {
    distinct_dates.get(window.saturating_sub(1)).copied()
}

fn date_range(start: Option<NaiveDate>, end: Option<NaiveDate>) -> Vec<Option<String>> {
    vec![
        start.map(|date| date.to_string()),
        end.map(|date| date.to_string()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_chunk_size_keeps_small_windows_together() {
        assert_eq!(resolve_chunk_size(60, None), 60);
    }

    #[test]
    fn automatic_chunk_size_splits_large_backfills() {
        assert_eq!(resolve_chunk_size(61, None), AUTO_CHUNK_SIZE_DAYS);
    }

    #[test]
    fn requested_chunk_size_overrides_auto() {
        assert_eq!(resolve_chunk_size(200, Some(45)), 45);
    }
}
