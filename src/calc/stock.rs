use std::{collections::HashSet, thread};

use chrono::NaiveDate;

use super::{
    index::compute_index_rows_for_ranges,
    left_peak::compute_left_peak_row,
    macd::{
        close_or_nan, compute_asof_row, finalize_period, MacdAccumulator, PeriodBuilder, PeriodKind,
    },
    model::{ComputedRows, PeriodStateRow, SourceBar},
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
    compute_index_rows_for_ranges(
        index_source_rows,
        &index_ranges,
        target_dates,
        &mut computed,
    );
    computed
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
