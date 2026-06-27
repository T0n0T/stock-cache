use std::{
    collections::{BTreeMap, HashSet},
    time::Instant,
};

use anyhow::Result;
use chrono::NaiveDate;

use super::{
    constants::{LEFT_PEAK_CALC_VERSION, MACD_CALC_VERSION, ROLLING_CALC_VERSION},
    coverage::has_period_state,
    model::{ComputedRows, PeriodStateRow},
};

const INSERT_PERIOD_STATE_SQL: &str = r#"
insert into "stock_period_indicator_state" (
    "ts_code", "period_type", "period_start_date", "period_end_date", "period_close_qfq",
    "ema12", "ema26", "dif", "dea", "hist_x2", "period_count", "calc_version"
) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
on conflict ("ts_code", "period_type", "period_end_date", "calc_version") do update set
    "period_start_date" = excluded."period_start_date",
    "period_close_qfq" = excluded."period_close_qfq",
    "ema12" = excluded."ema12",
    "ema26" = excluded."ema26",
    "dif" = excluded."dif",
    "dea" = excluded."dea",
    "hist_x2" = excluded."hist_x2",
    "period_count" = excluded."period_count"
"#;

const INSERT_INDEX_PERIOD_STATE_SQL: &str = r#"
insert into "index_period_indicator_state" (
    "ts_code", "period_type", "period_start_date", "period_end_date", "period_close",
    "ema12", "ema26", "dif", "dea", "hist_x2", "period_count", "calc_version"
) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
on conflict ("ts_code", "period_type", "period_end_date", "calc_version") do update set
    "period_start_date" = excluded."period_start_date",
    "period_close" = excluded."period_close",
    "ema12" = excluded."ema12",
    "ema26" = excluded."ema26",
    "dif" = excluded."dif",
    "dea" = excluded."dea",
    "hist_x2" = excluded."hist_x2",
    "period_count" = excluded."period_count"
"#;

const INSERT_ASOF_SQL: &str = r#"
insert into "stock_daily_asof_indicators" (
    "ts_code", "trade_date",
    "daily_dif_asof", "daily_dea_asof", "daily_hist_x2_asof", "daily_dea_pctile_asof", "daily_period_count",
    "weekly_dif_asof", "weekly_dea_asof", "weekly_hist_x2_asof", "weekly_dea_pctile_asof", "weekly_period_count",
    "monthly_dif_asof", "monthly_dea_asof", "monthly_hist_x2_asof", "monthly_dea_pctile_asof", "monthly_period_count",
    "calc_version"
) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
on conflict ("ts_code", "trade_date", "calc_version") do update set
    "daily_dif_asof" = excluded."daily_dif_asof",
    "daily_dea_asof" = excluded."daily_dea_asof",
    "daily_hist_x2_asof" = excluded."daily_hist_x2_asof",
    "daily_dea_pctile_asof" = excluded."daily_dea_pctile_asof",
    "daily_period_count" = excluded."daily_period_count",
    "weekly_dif_asof" = excluded."weekly_dif_asof",
    "weekly_dea_asof" = excluded."weekly_dea_asof",
    "weekly_hist_x2_asof" = excluded."weekly_hist_x2_asof",
    "weekly_dea_pctile_asof" = excluded."weekly_dea_pctile_asof",
    "weekly_period_count" = excluded."weekly_period_count",
    "monthly_dif_asof" = excluded."monthly_dif_asof",
    "monthly_dea_asof" = excluded."monthly_dea_asof",
    "monthly_hist_x2_asof" = excluded."monthly_hist_x2_asof",
    "monthly_dea_pctile_asof" = excluded."monthly_dea_pctile_asof",
    "monthly_period_count" = excluded."monthly_period_count"
"#;

const INSERT_INDEX_ASOF_SQL: &str = r#"
insert into "index_daily_asof_indicators" (
    "ts_code", "trade_date",
    "daily_dif_asof", "daily_dea_asof", "daily_hist_x2_asof", "daily_dea_pctile_asof", "daily_period_count",
    "weekly_dif_asof", "weekly_dea_asof", "weekly_hist_x2_asof", "weekly_dea_pctile_asof", "weekly_period_count",
    "monthly_dif_asof", "monthly_dea_asof", "monthly_hist_x2_asof", "monthly_dea_pctile_asof", "monthly_period_count",
    "calc_version"
) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
on conflict ("ts_code", "trade_date", "calc_version") do update set
    "daily_dif_asof" = excluded."daily_dif_asof",
    "daily_dea_asof" = excluded."daily_dea_asof",
    "daily_hist_x2_asof" = excluded."daily_hist_x2_asof",
    "daily_dea_pctile_asof" = excluded."daily_dea_pctile_asof",
    "daily_period_count" = excluded."daily_period_count",
    "weekly_dif_asof" = excluded."weekly_dif_asof",
    "weekly_dea_asof" = excluded."weekly_dea_asof",
    "weekly_hist_x2_asof" = excluded."weekly_hist_x2_asof",
    "weekly_dea_pctile_asof" = excluded."weekly_dea_pctile_asof",
    "weekly_period_count" = excluded."weekly_period_count",
    "monthly_dif_asof" = excluded."monthly_dif_asof",
    "monthly_dea_asof" = excluded."monthly_dea_asof",
    "monthly_hist_x2_asof" = excluded."monthly_hist_x2_asof",
    "monthly_dea_pctile_asof" = excluded."monthly_dea_pctile_asof",
    "monthly_period_count" = excluded."monthly_period_count"
"#;

const INSERT_ROLLING_SQL: &str = r#"
insert into "stock_daily_rolling_factors" (
    "ts_code", "trade_date",
    "ma25_qfq", "ma144_qfq", "ma220_qfq",
    "high_20_qfq", "high_90_qfq", "low_90_qfq", "high_120_qfq", "low_120_qfq",
    "close_to_20d_max_close_pct", "position_90d", "position_120d",
    "volume_ma5", "volume_ma20", "volume_to_ma5_ratio", "volume_to_ma20_ratio", "volume_ma5_to_ma20_ratio",
    "turnover_rate_ma5", "turnover_to_ma5_ratio",
    "range_compression_20d", "range_compression_40d",
    "calc_version"
) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23)
on conflict ("ts_code", "trade_date", "calc_version") do update set
    "ma25_qfq" = excluded."ma25_qfq",
    "ma144_qfq" = excluded."ma144_qfq",
    "ma220_qfq" = excluded."ma220_qfq",
    "high_20_qfq" = excluded."high_20_qfq",
    "high_90_qfq" = excluded."high_90_qfq",
    "low_90_qfq" = excluded."low_90_qfq",
    "high_120_qfq" = excluded."high_120_qfq",
    "low_120_qfq" = excluded."low_120_qfq",
    "close_to_20d_max_close_pct" = excluded."close_to_20d_max_close_pct",
    "position_90d" = excluded."position_90d",
    "position_120d" = excluded."position_120d",
    "volume_ma5" = excluded."volume_ma5",
    "volume_ma20" = excluded."volume_ma20",
    "volume_to_ma5_ratio" = excluded."volume_to_ma5_ratio",
    "volume_to_ma20_ratio" = excluded."volume_to_ma20_ratio",
    "volume_ma5_to_ma20_ratio" = excluded."volume_ma5_to_ma20_ratio",
    "turnover_rate_ma5" = excluded."turnover_rate_ma5",
    "turnover_to_ma5_ratio" = excluded."turnover_to_ma5_ratio",
    "range_compression_20d" = excluded."range_compression_20d",
    "range_compression_40d" = excluded."range_compression_40d"
"#;

const INSERT_LEFT_PEAK_SQL: &str = r#"
insert into "stock_daily_left_peak" (
    "ts_code", "trade_date", "left_peak_date", "left_peak_high", "breakout_date", "breakout_close",
    "breakout_body_above_left_peak", "first_bear_date", "first_bear_open", "first_bear_missing",
    "pick_close", "b_div_a", "abs_ba_minus_1", "a_lt_b", "is_valid", "status", "calc_version"
) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
on conflict ("ts_code", "trade_date", "calc_version") do update set
    "left_peak_date" = excluded."left_peak_date",
    "left_peak_high" = excluded."left_peak_high",
    "breakout_date" = excluded."breakout_date",
    "breakout_close" = excluded."breakout_close",
    "breakout_body_above_left_peak" = excluded."breakout_body_above_left_peak",
    "first_bear_date" = excluded."first_bear_date",
    "first_bear_open" = excluded."first_bear_open",
    "first_bear_missing" = excluded."first_bear_missing",
    "pick_close" = excluded."pick_close",
    "b_div_a" = excluded."b_div_a",
    "abs_ba_minus_1" = excluded."abs_ba_minus_1",
    "a_lt_b" = excluded."a_lt_b",
    "is_valid" = excluded."is_valid",
    "status" = excluded."status"
"#;

pub(super) async fn write_calc_rows(
    client: &mut tokio_postgres::Client,
    computed: &ComputedRows,
    existing_period_state_keys: &HashSet<(String, String, NaiveDate)>,
) -> Result<BTreeMap<String, usize>> {
    let transaction = client.transaction().await?;
    let period_statement = transaction.prepare(INSERT_PERIOD_STATE_SQL).await?;
    let index_period_statement = transaction.prepare(INSERT_INDEX_PERIOD_STATE_SQL).await?;
    let asof_statement = transaction.prepare(INSERT_ASOF_SQL).await?;
    let index_asof_statement = transaction.prepare(INSERT_INDEX_ASOF_SQL).await?;
    let rolling_statement = transaction.prepare(INSERT_ROLLING_SQL).await?;
    let left_peak_statement = transaction.prepare(INSERT_LEFT_PEAK_SQL).await?;

    let mut period_rows = computed.period_states.iter().collect::<Vec<_>>();
    period_rows.retain(|row| !has_period_state(existing_period_state_keys, row));
    period_rows.sort_by(|left, right| {
        left.period_end_date
            .cmp(&right.period_end_date)
            .then_with(|| left.period_type.cmp(&right.period_type))
            .then_with(|| left.ts_code.cmp(&right.ts_code))
    });
    let period_groups = count_period_state_groups(&period_rows);
    let started = Instant::now();
    eprintln!(
        "[calc] write stock_period_indicator_state start rows={} date_groups={}",
        period_rows.len(),
        period_groups
    );
    let mut completed_groups = 0;
    let mut group_rows = 0;
    for (index, row) in period_rows.iter().enumerate() {
        transaction
            .execute(
                &period_statement,
                &[
                    &row.ts_code,
                    &row.period_type,
                    &row.period_start_date,
                    &row.period_end_date,
                    &row.period_close_qfq,
                    &row.ema12,
                    &row.ema26,
                    &row.dif,
                    &row.dea,
                    &row.hist_x2,
                    &row.period_count,
                    &MACD_CALC_VERSION,
                ],
            )
            .await?;
        group_rows += 1;
        let is_group_done = period_rows.get(index + 1).is_none_or(|next| {
            next.period_end_date != row.period_end_date || next.period_type != row.period_type
        });
        if is_group_done {
            completed_groups += 1;
            log_calc_date_group_progress(CalcDateGroupProgress {
                table: "stock_period_indicator_state",
                date_label: "period_end_date",
                date: row.period_end_date,
                period_type: Some(row.period_type.as_str()),
                group_rows,
                completed_groups,
                total_groups: period_groups,
                written_rows: index + 1,
                total_rows: period_rows.len(),
                started,
            });
            group_rows = 0;
        }
    }
    eprintln!(
        "[calc] write stock_period_indicator_state done rows={} elapsed={:.1}s",
        period_rows.len(),
        started.elapsed().as_secs_f64()
    );

    let mut index_period_rows = computed.index_period_states.iter().collect::<Vec<_>>();
    index_period_rows.sort_by(|left, right| {
        left.period_end_date
            .cmp(&right.period_end_date)
            .then_with(|| left.period_type.cmp(&right.period_type))
            .then_with(|| left.ts_code.cmp(&right.ts_code))
    });
    let index_period_groups = count_period_state_groups(&index_period_rows);
    let started = Instant::now();
    eprintln!(
        "[calc] write index_period_indicator_state start rows={} date_groups={}",
        index_period_rows.len(),
        index_period_groups
    );
    let mut completed_groups = 0;
    let mut group_rows = 0;
    for (index, row) in index_period_rows.iter().enumerate() {
        transaction
            .execute(
                &index_period_statement,
                &[
                    &row.ts_code,
                    &row.period_type,
                    &row.period_start_date,
                    &row.period_end_date,
                    &row.period_close_qfq,
                    &row.ema12,
                    &row.ema26,
                    &row.dif,
                    &row.dea,
                    &row.hist_x2,
                    &row.period_count,
                    &MACD_CALC_VERSION,
                ],
            )
            .await?;
        group_rows += 1;
        let is_group_done = index_period_rows.get(index + 1).is_none_or(|next| {
            next.period_end_date != row.period_end_date || next.period_type != row.period_type
        });
        if is_group_done {
            completed_groups += 1;
            log_calc_date_group_progress(CalcDateGroupProgress {
                table: "index_period_indicator_state",
                date_label: "period_end_date",
                date: row.period_end_date,
                period_type: Some(row.period_type.as_str()),
                group_rows,
                completed_groups,
                total_groups: index_period_groups,
                written_rows: index + 1,
                total_rows: index_period_rows.len(),
                started,
            });
            group_rows = 0;
        }
    }
    eprintln!(
        "[calc] write index_period_indicator_state done rows={} elapsed={:.1}s",
        index_period_rows.len(),
        started.elapsed().as_secs_f64()
    );

    let mut asof_rows = computed.asof_rows.iter().collect::<Vec<_>>();
    asof_rows.sort_by(|left, right| {
        left.trade_date
            .cmp(&right.trade_date)
            .then_with(|| left.ts_code.cmp(&right.ts_code))
    });
    let asof_groups = count_trade_date_groups(&asof_rows, |row| row.trade_date);
    let started = Instant::now();
    eprintln!(
        "[calc] write stock_daily_asof_indicators start rows={} trade_date_groups={}",
        asof_rows.len(),
        asof_groups
    );
    let mut completed_groups = 0;
    let mut group_rows = 0;
    for (index, row) in asof_rows.iter().enumerate() {
        transaction
            .execute(
                &asof_statement,
                &[
                    &row.ts_code,
                    &row.trade_date,
                    &row.daily_dif_asof,
                    &row.daily_dea_asof,
                    &row.daily_hist_x2_asof,
                    &row.daily_dea_pctile_asof,
                    &row.daily_period_count,
                    &row.weekly_dif_asof,
                    &row.weekly_dea_asof,
                    &row.weekly_hist_x2_asof,
                    &row.weekly_dea_pctile_asof,
                    &row.weekly_period_count,
                    &row.monthly_dif_asof,
                    &row.monthly_dea_asof,
                    &row.monthly_hist_x2_asof,
                    &row.monthly_dea_pctile_asof,
                    &row.monthly_period_count,
                    &MACD_CALC_VERSION,
                ],
            )
            .await?;
        group_rows += 1;
        let is_group_done = asof_rows
            .get(index + 1)
            .is_none_or(|next| next.trade_date != row.trade_date);
        if is_group_done {
            completed_groups += 1;
            log_calc_date_group_progress(CalcDateGroupProgress {
                table: "stock_daily_asof_indicators",
                date_label: "trade_date",
                date: row.trade_date,
                period_type: None,
                group_rows,
                completed_groups,
                total_groups: asof_groups,
                written_rows: index + 1,
                total_rows: asof_rows.len(),
                started,
            });
            group_rows = 0;
        }
    }
    eprintln!(
        "[calc] write stock_daily_asof_indicators done rows={} elapsed={:.1}s",
        asof_rows.len(),
        started.elapsed().as_secs_f64()
    );

    let mut index_asof_rows = computed.index_asof_rows.iter().collect::<Vec<_>>();
    index_asof_rows.sort_by(|left, right| {
        left.trade_date
            .cmp(&right.trade_date)
            .then_with(|| left.ts_code.cmp(&right.ts_code))
    });
    let index_asof_groups = count_trade_date_groups(&index_asof_rows, |row| row.trade_date);
    let started = Instant::now();
    eprintln!(
        "[calc] write index_daily_asof_indicators start rows={} trade_date_groups={}",
        index_asof_rows.len(),
        index_asof_groups
    );
    let mut completed_groups = 0;
    let mut group_rows = 0;
    for (index, row) in index_asof_rows.iter().enumerate() {
        transaction
            .execute(
                &index_asof_statement,
                &[
                    &row.ts_code,
                    &row.trade_date,
                    &row.daily_dif_asof,
                    &row.daily_dea_asof,
                    &row.daily_hist_x2_asof,
                    &row.daily_dea_pctile_asof,
                    &row.daily_period_count,
                    &row.weekly_dif_asof,
                    &row.weekly_dea_asof,
                    &row.weekly_hist_x2_asof,
                    &row.weekly_dea_pctile_asof,
                    &row.weekly_period_count,
                    &row.monthly_dif_asof,
                    &row.monthly_dea_asof,
                    &row.monthly_hist_x2_asof,
                    &row.monthly_dea_pctile_asof,
                    &row.monthly_period_count,
                    &MACD_CALC_VERSION,
                ],
            )
            .await?;
        group_rows += 1;
        let is_group_done = index_asof_rows
            .get(index + 1)
            .is_none_or(|next| next.trade_date != row.trade_date);
        if is_group_done {
            completed_groups += 1;
            log_calc_date_group_progress(CalcDateGroupProgress {
                table: "index_daily_asof_indicators",
                date_label: "trade_date",
                date: row.trade_date,
                period_type: None,
                group_rows,
                completed_groups,
                total_groups: index_asof_groups,
                written_rows: index + 1,
                total_rows: index_asof_rows.len(),
                started,
            });
            group_rows = 0;
        }
    }
    eprintln!(
        "[calc] write index_daily_asof_indicators done rows={} elapsed={:.1}s",
        index_asof_rows.len(),
        started.elapsed().as_secs_f64()
    );

    let mut rolling_rows = computed.rolling_rows.iter().collect::<Vec<_>>();
    rolling_rows.sort_by(|left, right| {
        left.trade_date
            .cmp(&right.trade_date)
            .then_with(|| left.ts_code.cmp(&right.ts_code))
    });
    let rolling_groups = count_trade_date_groups(&rolling_rows, |row| row.trade_date);
    let started = Instant::now();
    eprintln!(
        "[calc] write stock_daily_rolling_factors start rows={} trade_date_groups={}",
        rolling_rows.len(),
        rolling_groups
    );
    let mut completed_groups = 0;
    let mut group_rows = 0;
    for (index, row) in rolling_rows.iter().enumerate() {
        transaction
            .execute(
                &rolling_statement,
                &[
                    &row.ts_code,
                    &row.trade_date,
                    &row.ma25_qfq,
                    &row.ma144_qfq,
                    &row.ma220_qfq,
                    &row.high_20_qfq,
                    &row.high_90_qfq,
                    &row.low_90_qfq,
                    &row.high_120_qfq,
                    &row.low_120_qfq,
                    &row.close_to_20d_max_close_pct,
                    &row.position_90d,
                    &row.position_120d,
                    &row.volume_ma5,
                    &row.volume_ma20,
                    &row.volume_to_ma5_ratio,
                    &row.volume_to_ma20_ratio,
                    &row.volume_ma5_to_ma20_ratio,
                    &row.turnover_rate_ma5,
                    &row.turnover_to_ma5_ratio,
                    &row.range_compression_20d,
                    &row.range_compression_40d,
                    &ROLLING_CALC_VERSION,
                ],
            )
            .await?;
        group_rows += 1;
        let is_group_done = rolling_rows
            .get(index + 1)
            .is_none_or(|next| next.trade_date != row.trade_date);
        if is_group_done {
            completed_groups += 1;
            log_calc_date_group_progress(CalcDateGroupProgress {
                table: "stock_daily_rolling_factors",
                date_label: "trade_date",
                date: row.trade_date,
                period_type: None,
                group_rows,
                completed_groups,
                total_groups: rolling_groups,
                written_rows: index + 1,
                total_rows: rolling_rows.len(),
                started,
            });
            group_rows = 0;
        }
    }
    eprintln!(
        "[calc] write stock_daily_rolling_factors done rows={} elapsed={:.1}s",
        rolling_rows.len(),
        started.elapsed().as_secs_f64()
    );

    let mut left_peak_rows = computed.left_peak_rows.iter().collect::<Vec<_>>();
    left_peak_rows.sort_by(|left, right| {
        left.trade_date
            .cmp(&right.trade_date)
            .then_with(|| left.ts_code.cmp(&right.ts_code))
    });
    let left_peak_groups = count_trade_date_groups(&left_peak_rows, |row| row.trade_date);
    let started = Instant::now();
    eprintln!(
        "[calc] write stock_daily_left_peak start rows={} trade_date_groups={}",
        left_peak_rows.len(),
        left_peak_groups
    );
    let mut completed_groups = 0;
    let mut group_rows = 0;
    for (index, row) in left_peak_rows.iter().enumerate() {
        transaction
            .execute(
                &left_peak_statement,
                &[
                    &row.ts_code,
                    &row.trade_date,
                    &row.left_peak_date,
                    &row.left_peak_high,
                    &row.breakout_date,
                    &row.breakout_close,
                    &row.breakout_body_above_left_peak,
                    &row.first_bear_date,
                    &row.first_bear_open,
                    &row.first_bear_missing,
                    &row.pick_close,
                    &row.b_div_a,
                    &row.abs_ba_minus_1,
                    &row.a_lt_b,
                    &row.is_valid,
                    &row.status,
                    &LEFT_PEAK_CALC_VERSION,
                ],
            )
            .await?;
        group_rows += 1;
        let is_group_done = left_peak_rows
            .get(index + 1)
            .is_none_or(|next| next.trade_date != row.trade_date);
        if is_group_done {
            completed_groups += 1;
            log_calc_date_group_progress(CalcDateGroupProgress {
                table: "stock_daily_left_peak",
                date_label: "trade_date",
                date: row.trade_date,
                period_type: None,
                group_rows,
                completed_groups,
                total_groups: left_peak_groups,
                written_rows: index + 1,
                total_rows: left_peak_rows.len(),
                started,
            });
            group_rows = 0;
        }
    }
    eprintln!(
        "[calc] write stock_daily_left_peak done rows={} elapsed={:.1}s",
        left_peak_rows.len(),
        started.elapsed().as_secs_f64()
    );

    eprintln!("[calc] commit transaction start");
    let started = Instant::now();
    transaction.commit().await?;
    eprintln!(
        "[calc] commit transaction done elapsed={:.1}s",
        started.elapsed().as_secs_f64()
    );

    Ok(BTreeMap::from([
        (
            "stock_period_indicator_state".to_string(),
            period_rows.len(),
        ),
        (
            "index_period_indicator_state".to_string(),
            index_period_rows.len(),
        ),
        (
            "stock_daily_asof_indicators".to_string(),
            computed.asof_rows.len(),
        ),
        (
            "index_daily_asof_indicators".to_string(),
            computed.index_asof_rows.len(),
        ),
        (
            "stock_daily_rolling_factors".to_string(),
            computed.rolling_rows.len(),
        ),
        (
            "stock_daily_left_peak".to_string(),
            computed.left_peak_rows.len(),
        ),
    ]))
}

struct CalcDateGroupProgress<'a> {
    table: &'a str,
    date_label: &'a str,
    date: NaiveDate,
    period_type: Option<&'a str>,
    group_rows: usize,
    completed_groups: usize,
    total_groups: usize,
    written_rows: usize,
    total_rows: usize,
    started: Instant,
}

fn log_calc_date_group_progress(progress: CalcDateGroupProgress<'_>) {
    if let Some(period_type) = progress.period_type {
        eprintln!(
            "[calc] write {} {}={} period_type={} group_rows={} date_group={}/{} rows={}/{} elapsed={:.1}s",
            progress.table,
            progress.date_label,
            progress.date.format("%Y%m%d"),
            period_type,
            progress.group_rows,
            progress.completed_groups,
            progress.total_groups,
            progress.written_rows,
            progress.total_rows,
            progress.started.elapsed().as_secs_f64()
        );
    } else {
        eprintln!(
            "[calc] write {} {}={} group_rows={} date_group={}/{} rows={}/{} elapsed={:.1}s",
            progress.table,
            progress.date_label,
            progress.date.format("%Y%m%d"),
            progress.group_rows,
            progress.completed_groups,
            progress.total_groups,
            progress.written_rows,
            progress.total_rows,
            progress.started.elapsed().as_secs_f64()
        );
    }
}

fn count_period_state_groups(rows: &[&PeriodStateRow]) -> usize {
    rows.iter()
        .fold((0, None::<(NaiveDate, &str)>), |(count, previous), row| {
            let current = (row.period_end_date, row.period_type.as_str());
            if previous == Some(current) {
                (count, previous)
            } else {
                (count + 1, Some(current))
            }
        })
        .0
}

fn count_trade_date_groups<T>(rows: &[&T], date: fn(&T) -> NaiveDate) -> usize {
    rows.iter()
        .fold((0, None::<NaiveDate>), |(count, previous), row| {
            let current = date(row);
            if previous == Some(current) {
                (count, previous)
            } else {
                (count + 1, Some(current))
            }
        })
        .0
}
