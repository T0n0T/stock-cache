use std::{collections::HashSet, thread};

use anyhow::{anyhow, Result};
use chrono::NaiveDate;

use super::{
    czsc::{CzscAnalyzer, CzscConfig, CzscSourceBar},
    index::compute_index_rows_for_ranges,
    left_peak::compute_left_peak_row,
    macd::{
        close_or_nan, compute_asof_row, finalize_period, MacdAccumulator, PeriodBuilder, PeriodKind,
    },
    model::{ComputedRows, CzscStructureRow, PeriodStateRow, SourceBar},
    rolling::compute_rolling_row,
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
) -> Result<ComputedRows> {
    let stock_ranges_for_source = stock_ranges(source_rows);
    let index_ranges = stock_ranges(index_source_rows);
    let concurrency = (max_concurrency.max(1) as usize).min(stock_ranges_for_source.len().max(1));
    let mut computed = if concurrency <= 1 || stock_ranges_for_source.len() <= 1 {
        compute_calc_rows_for_ranges(source_rows, &stock_ranges_for_source, target_dates)?
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

        thread::scope(|scope| -> Result<ComputedRows> {
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
                let rows = handle
                    .join()
                    .map_err(|_| anyhow!("calc worker thread panicked"))??;
                merged.append(rows);
            }
            Ok(merged)
        })?
    };
    compute_index_rows_for_ranges(
        index_source_rows,
        &index_ranges,
        target_dates,
        &mut computed,
    );
    Ok(computed)
}

pub(super) fn compute_czsc_rows(
    source_rows: &[SourceBar],
    target_dates: &[NaiveDate],
    max_concurrency: u32,
) -> Result<Vec<CzscStructureRow>> {
    let ranges = stock_ranges(source_rows);
    let concurrency = (max_concurrency.max(1) as usize).min(ranges.len().max(1));
    if concurrency <= 1 || ranges.len() <= 1 {
        return compute_czsc_rows_for_ranges(source_rows, &ranges, target_dates);
    }

    let chunk_size = ranges.len().div_ceil(concurrency);
    let chunks = ranges.chunks(chunk_size).collect::<Vec<_>>();
    eprintln!(
        "[czsc] compute rows parallel stocks={} workers={}",
        ranges.len(),
        chunks.len()
    );
    thread::scope(|scope| -> Result<Vec<CzscStructureRow>> {
        let handles = chunks
            .into_iter()
            .map(|chunk| {
                scope.spawn(move || compute_czsc_rows_for_ranges(source_rows, chunk, target_dates))
            })
            .collect::<Vec<_>>();
        let mut rows = Vec::new();
        for handle in handles {
            rows.extend(
                handle
                    .join()
                    .map_err(|_| anyhow!("CZSC calc worker thread panicked"))??,
            );
        }
        Ok(rows)
    })
}

fn compute_czsc_rows_for_ranges(
    source_rows: &[SourceBar],
    stock_ranges: &[(usize, usize)],
    target_dates: &[NaiveDate],
) -> Result<Vec<CzscStructureRow>> {
    let target_dates = target_dates.iter().copied().collect::<HashSet<_>>();
    let mut rows = Vec::new();
    for (start, end) in stock_ranges {
        let mut analyzer = CzscAnalyzer::new(CzscConfig {
            min_bi_len: 6,
            max_bi_num: 50,
        });
        for bar in &source_rows[*start..*end] {
            let row = compute_czsc_row(&mut analyzer, bar)?;
            if target_dates.contains(&bar.trade_date) {
                rows.push(row);
            }
        }
    }
    Ok(rows)
}

pub(super) fn compute_calc_rows_for_ranges(
    source_rows: &[SourceBar],
    stock_ranges: &[(usize, usize)],
    target_dates: &[NaiveDate],
) -> Result<ComputedRows> {
    let target_dates = target_dates.iter().copied().collect::<HashSet<_>>();
    let mut computed = ComputedRows::default();
    for (start, end) in stock_ranges {
        compute_stock_rows(&source_rows[*start..*end], &target_dates, &mut computed)?;
    }
    Ok(computed)
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
) -> Result<()> {
    let Some(first) = bars.first() else {
        return Ok(());
    };
    let mut daily_macd = MacdAccumulator::default();
    let mut weekly_macd = MacdAccumulator::default();
    let mut monthly_macd = MacdAccumulator::default();
    let mut current_week = PeriodBuilder::new(PeriodKind::Weekly, first.trade_date);
    let mut current_month = PeriodBuilder::new(PeriodKind::Monthly, first.trade_date);
    let mut daily_deas = Vec::new();
    let mut weekly_deas = Vec::new();
    let mut monthly_deas = Vec::new();
    let mut czsc = CzscAnalyzer::new(CzscConfig {
        min_bi_len: 6,
        max_bi_num: 50,
    });
    for (idx, bar) in bars.iter().enumerate() {
        let czsc_row = compute_czsc_row(&mut czsc, bar)?;
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
            computed.czsc_rows.push(czsc_row);
        }
    }
    Ok(())
}

fn compute_czsc_row(analyzer: &mut CzscAnalyzer, bar: &SourceBar) -> Result<CzscStructureRow> {
    let snapshot = analyzer.process(CzscSourceBar {
        trade_date: bar.trade_date,
        open: bar.open_qfq,
        high: bar.high_qfq,
        low: bar.low_qfq,
        close: bar.close_qfq,
    })?;
    Ok(CzscStructureRow::from_snapshot(
        &bar.ts_code,
        bar.trade_date,
        snapshot,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 1, day).expect("test date")
    }

    #[test]
    fn czsc_rows_use_full_history_but_only_emit_target_dates() {
        let bars = (1..=10)
            .map(|day| SourceBar {
                ts_code: "000001.SZ".to_string(),
                trade_date: date(day),
                open_qfq: Some(10.0 + f64::from(day)),
                high_qfq: Some(11.0 + f64::from(day)),
                low_qfq: Some(9.0 + f64::from(day)),
                close_qfq: Some(10.5 + f64::from(day)),
                vol: Some(1.0),
                turnover_rate: Some(1.0),
            })
            .collect::<Vec<_>>();
        let mut bars = bars;
        bars[9].high_qfq = Some(1.0);
        let computed = compute_calc_rows(&bars, &[], &[date(9), date(10)], 1)
            .expect("valid ordered stock history");

        assert_eq!(computed.czsc_rows.len(), 2);
        assert_eq!(computed.czsc_rows[0].trade_date, date(9));
        assert_eq!(computed.czsc_rows[1].trade_date, date(10));
        assert!(computed.czsc_rows[0].source_bar_valid);
        assert_eq!(computed.czsc_rows[1].status, "invalid_source_bar");
        assert_eq!(computed.left_peak_rows.len(), 2);
    }

    #[test]
    fn dedicated_czsc_calculation_only_emits_requested_source_dates() {
        let mut bars = Vec::new();
        for ts_code in ["000001.SZ", "000002.SZ"] {
            bars.extend((1..=10).map(|day| SourceBar {
                ts_code: ts_code.to_string(),
                trade_date: date(day),
                open_qfq: Some(10.0 + f64::from(day)),
                high_qfq: Some(11.0 + f64::from(day)),
                low_qfq: Some(9.0 + f64::from(day)),
                close_qfq: Some(10.5 + f64::from(day)),
                vol: Some(1.0),
                turnover_rate: Some(1.0),
            }));
        }
        bars[9].high_qfq = Some(1.0);

        let rows = compute_czsc_rows(&bars, &[date(9), date(10)], 2)
            .expect("valid ordered stock histories");

        assert_eq!(rows.len(), 4);
        assert_eq!(
            rows.iter()
                .map(|row| (row.ts_code.as_str(), row.trade_date))
                .collect::<Vec<_>>(),
            vec![
                ("000001.SZ", date(9)),
                ("000001.SZ", date(10)),
                ("000002.SZ", date(9)),
                ("000002.SZ", date(10)),
            ]
        );
        assert_eq!(rows[1].status, "invalid_source_bar");
    }
}
