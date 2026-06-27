use std::collections::HashSet;

use chrono::NaiveDate;

use super::{
    macd::{
        close_or_nan, compute_asof_row, finalize_period, MacdAccumulator, PeriodBuilder, PeriodKind,
    },
    model::{ComputedRows, PeriodStateRow, SourceBar},
};

pub(super) fn compute_index_rows_for_ranges(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_index_asof_rows_for_target_dates() {
        let bars = vec![
            bar("885001.TI", 2025, 1, 2, 10.0),
            bar("885001.TI", 2025, 1, 3, 11.0),
            bar("885001.TI", 2025, 1, 6, 12.0),
        ];
        let target_dates = vec![NaiveDate::from_ymd_opt(2025, 1, 6).unwrap()];
        let mut computed = ComputedRows::default();

        compute_index_rows_for_ranges(&bars, &[(0, bars.len())], &target_dates, &mut computed);

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
