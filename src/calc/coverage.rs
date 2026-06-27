use std::collections::{BTreeMap, HashSet};

use anyhow::Result;
use chrono::NaiveDate;

use super::{
    constants::{LEFT_PEAK_CALC_VERSION, MACD_CALC_VERSION, ROLLING_CALC_VERSION},
    model::PeriodStateRow,
};

#[derive(Debug, Default)]
pub(super) struct ExistingCalcCoverage {
    source_counts_by_date: BTreeMap<NaiveDate, i64>,
    index_source_counts_by_date: BTreeMap<NaiveDate, i64>,
    index_period_daily_counts_by_date: BTreeMap<NaiveDate, i64>,
    asof_counts_by_date: BTreeMap<NaiveDate, i64>,
    rolling_counts_by_date: BTreeMap<NaiveDate, i64>,
    left_peak_counts_by_date: BTreeMap<NaiveDate, i64>,
    index_asof_counts_by_date: BTreeMap<NaiveDate, i64>,
}

impl ExistingCalcCoverage {
    pub(super) fn is_target_trade_date_complete(&self, trade_date: NaiveDate) -> bool {
        let Some(source_count) = self.source_counts_by_date.get(&trade_date).copied() else {
            return false;
        };
        let stock_complete = source_count > 0
            && self
                .asof_counts_by_date
                .get(&trade_date)
                .copied()
                .unwrap_or(0)
                >= source_count
            && self
                .rolling_counts_by_date
                .get(&trade_date)
                .copied()
                .unwrap_or(0)
                >= source_count
            && self
                .left_peak_counts_by_date
                .get(&trade_date)
                .copied()
                .unwrap_or(0)
                >= source_count;
        let index_complete = match self.index_source_counts_by_date.get(&trade_date).copied() {
            Some(index_source_count) if index_source_count > 0 => {
                self.index_period_daily_counts_by_date
                    .get(&trade_date)
                    .copied()
                    .unwrap_or(0)
                    >= index_source_count
                    && self
                        .index_asof_counts_by_date
                        .get(&trade_date)
                        .copied()
                        .unwrap_or(0)
                        >= index_source_count
            }
            _ => true,
        };
        stock_complete && index_complete
    }
}

pub(super) async fn load_existing_calc_coverage(
    client: &tokio_postgres::Client,
    target_dates: &[NaiveDate],
) -> Result<ExistingCalcCoverage> {
    let Some(first_target_date) = target_dates.first().copied() else {
        return Ok(ExistingCalcCoverage::default());
    };
    let Some(last_target_date) = target_dates.last().copied() else {
        return Ok(ExistingCalcCoverage::default());
    };
    let source_counts_by_date = load_date_counts(
        client,
        r#"
        select "trade_date", count(*)::bigint
        from "stock_stk_factor_pro"
        where "trade_date" between $1 and $2
        group by "trade_date"
        "#,
        first_target_date,
        last_target_date,
    )
    .await?;
    let index_source_counts_by_date = load_date_counts(
        client,
        r#"
        select "trade_date", count(*)::bigint
        from (
            select "trade_date"
            from "index_ths_daily"
            where "trade_date" between $1 and $2
            union all
            select "trade_date"
            from "index_idx_factor_pro"
            where "trade_date" between $1 and $2
        ) index_source
        group by "trade_date"
        "#,
        first_target_date,
        last_target_date,
    )
    .await?;
    let asof_counts_by_date = load_versioned_date_counts(
        client,
        r#"
        select "trade_date", count(*)::bigint
        from "stock_daily_asof_indicators"
        where "trade_date" between $1 and $2 and "calc_version" = $3
        group by "trade_date"
        "#,
        first_target_date,
        last_target_date,
        MACD_CALC_VERSION,
    )
    .await?;
    let rolling_counts_by_date = load_versioned_date_counts(
        client,
        r#"
        select "trade_date", count(*)::bigint
        from "stock_daily_rolling_factors"
        where "trade_date" between $1 and $2 and "calc_version" = $3
        group by "trade_date"
        "#,
        first_target_date,
        last_target_date,
        ROLLING_CALC_VERSION,
    )
    .await?;
    let left_peak_counts_by_date = load_versioned_date_counts(
        client,
        r#"
        select "trade_date", count(*)::bigint
        from "stock_daily_left_peak"
        where "trade_date" between $1 and $2 and "calc_version" = $3
        group by "trade_date"
        "#,
        first_target_date,
        last_target_date,
        LEFT_PEAK_CALC_VERSION,
    )
    .await?;
    let index_asof_counts_by_date = load_versioned_date_counts(
        client,
        r#"
        select "trade_date", count(*)::bigint
        from "index_daily_asof_indicators"
        where "trade_date" between $1 and $2 and "calc_version" = $3
        group by "trade_date"
        "#,
        first_target_date,
        last_target_date,
        MACD_CALC_VERSION,
    )
    .await?;
    let index_period_daily_counts_by_date = load_versioned_date_counts(
        client,
        r#"
        select "period_end_date", count(*)::bigint
        from "index_period_indicator_state"
        where "period_end_date" between $1 and $2
          and "period_type" = 'daily'
          and "calc_version" = $3
        group by "period_end_date"
        "#,
        first_target_date,
        last_target_date,
        MACD_CALC_VERSION,
    )
    .await?;
    Ok(ExistingCalcCoverage {
        source_counts_by_date,
        index_source_counts_by_date,
        index_period_daily_counts_by_date,
        asof_counts_by_date,
        rolling_counts_by_date,
        left_peak_counts_by_date,
        index_asof_counts_by_date,
    })
}

async fn load_date_counts(
    client: &tokio_postgres::Client,
    sql: &str,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Result<BTreeMap<NaiveDate, i64>> {
    Ok(client
        .query(sql, &[&start_date, &end_date])
        .await?
        .into_iter()
        .map(|row| (row.get(0), row.get(1)))
        .collect())
}

async fn load_versioned_date_counts(
    client: &tokio_postgres::Client,
    sql: &str,
    start_date: NaiveDate,
    end_date: NaiveDate,
    calc_version: &str,
) -> Result<BTreeMap<NaiveDate, i64>> {
    Ok(client
        .query(sql, &[&start_date, &end_date, &calc_version])
        .await?
        .into_iter()
        .map(|row| (row.get(0), row.get(1)))
        .collect())
}

pub(super) async fn load_existing_period_state_keys(
    client: &tokio_postgres::Client,
    rows: &[PeriodStateRow],
) -> Result<HashSet<(String, String, NaiveDate)>> {
    if rows.is_empty() {
        return Ok(HashSet::new());
    }
    let min_period_end_date = rows
        .iter()
        .map(|row| row.period_end_date)
        .min()
        .expect("checked non-empty");
    let max_period_end_date = rows
        .iter()
        .map(|row| row.period_end_date)
        .max()
        .expect("checked non-empty");
    Ok(client
        .query(
            r#"
            select "ts_code", "period_type", "period_end_date"
            from "stock_period_indicator_state"
            where "period_end_date" between $1 and $2
              and "calc_version" = $3
            "#,
            &[
                &min_period_end_date,
                &max_period_end_date,
                &MACD_CALC_VERSION,
            ],
        )
        .await?
        .into_iter()
        .map(|row| (row.get(0), row.get(1), row.get(2)))
        .collect())
}

pub(super) fn has_period_state(
    existing_period_state_keys: &HashSet<(String, String, NaiveDate)>,
    row: &PeriodStateRow,
) -> bool {
    existing_period_state_keys.contains(&(
        row.ts_code.clone(),
        row.period_type.clone(),
        row.period_end_date,
    ))
}
