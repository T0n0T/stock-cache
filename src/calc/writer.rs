use std::{
    collections::{BTreeMap, HashSet},
    time::Instant,
};

use anyhow::Result;
use chrono::NaiveDate;

use super::{
    constants::{
        CZSC_STRUCTURE_CALC_VERSION, LEFT_PEAK_CALC_VERSION, MACD_CALC_VERSION,
        ROLLING_CALC_VERSION,
    },
    coverage::has_period_state,
    model::{ComputedRows, CzscStructureRow, PeriodStateRow},
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

const INSERT_CZSC_STRUCTURE_SQL: &str = r#"
insert into "stock_daily_czsc_structure" (
    "ts_code", "trade_date", "source_bar_valid", "status",
    "available_finished_bi_count", "recent_bi_count",
    "last_bi_direction", "last_bi_start_date", "last_bi_end_date",
    "last_bi_high", "last_bi_low", "last_bi_mid",
    "latest_zs_start_date", "latest_zs_end_date", "latest_zs_bi_count",
    "latest_zs_zg", "latest_zs_zd", "latest_zs_zz",
    "recent_peak_date", "recent_peak_high",
    "post_peak_first_bear_date", "post_peak_first_bear_open",
    "post_peak_first_bear_high", "post_peak_first_bear_low",
    "post_peak_first_bear_close", "post_peak_first_bear_missing",
    "calc_version"
) values (
    $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
    $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27
)
on conflict ("ts_code", "trade_date", "calc_version") do update set
    "source_bar_valid" = excluded."source_bar_valid",
    "status" = excluded."status",
    "available_finished_bi_count" = excluded."available_finished_bi_count",
    "recent_bi_count" = excluded."recent_bi_count",
    "last_bi_direction" = excluded."last_bi_direction",
    "last_bi_start_date" = excluded."last_bi_start_date",
    "last_bi_end_date" = excluded."last_bi_end_date",
    "last_bi_high" = excluded."last_bi_high",
    "last_bi_low" = excluded."last_bi_low",
    "last_bi_mid" = excluded."last_bi_mid",
    "latest_zs_start_date" = excluded."latest_zs_start_date",
    "latest_zs_end_date" = excluded."latest_zs_end_date",
    "latest_zs_bi_count" = excluded."latest_zs_bi_count",
    "latest_zs_zg" = excluded."latest_zs_zg",
    "latest_zs_zd" = excluded."latest_zs_zd",
    "latest_zs_zz" = excluded."latest_zs_zz",
    "recent_peak_date" = excluded."recent_peak_date",
    "recent_peak_high" = excluded."recent_peak_high",
    "post_peak_first_bear_date" = excluded."post_peak_first_bear_date",
    "post_peak_first_bear_open" = excluded."post_peak_first_bear_open",
    "post_peak_first_bear_high" = excluded."post_peak_first_bear_high",
    "post_peak_first_bear_low" = excluded."post_peak_first_bear_low",
    "post_peak_first_bear_close" = excluded."post_peak_first_bear_close",
    "post_peak_first_bear_missing" = excluded."post_peak_first_bear_missing"
"#;

pub(super) async fn write_calc_rows(
    client: &mut tokio_postgres::Client,
    computed: &ComputedRows,
    existing_stock_period_state_keys: &HashSet<(String, String, NaiveDate)>,
    existing_index_period_state_keys: &HashSet<(String, String, NaiveDate)>,
) -> Result<BTreeMap<String, usize>> {
    let transaction = client.transaction().await?;
    let period_statement = transaction.prepare(INSERT_PERIOD_STATE_SQL).await?;
    let index_period_statement = transaction.prepare(INSERT_INDEX_PERIOD_STATE_SQL).await?;
    let asof_statement = transaction.prepare(INSERT_ASOF_SQL).await?;
    let index_asof_statement = transaction.prepare(INSERT_INDEX_ASOF_SQL).await?;
    let rolling_statement = transaction.prepare(INSERT_ROLLING_SQL).await?;
    let left_peak_statement = transaction.prepare(INSERT_LEFT_PEAK_SQL).await?;

    let mut period_rows =
        pending_period_rows(&computed.period_states, existing_stock_period_state_keys);
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

    let mut index_period_rows = pending_non_zero_dif_period_rows(
        &computed.index_period_states,
        existing_index_period_state_keys,
    );
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

    write_czsc_rows_in_transaction(&transaction, &computed.czsc_rows).await?;

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
        (
            "stock_daily_czsc_structure".to_string(),
            computed.czsc_rows.len(),
        ),
    ]))
}

pub(super) async fn write_czsc_rows(
    client: &mut tokio_postgres::Client,
    rows: &[CzscStructureRow],
) -> Result<usize> {
    let transaction = client.transaction().await?;
    let written = write_czsc_rows_in_transaction(&transaction, rows).await?;
    transaction.commit().await?;
    Ok(written)
}

async fn write_czsc_rows_in_transaction(
    transaction: &tokio_postgres::Transaction<'_>,
    rows: &[CzscStructureRow],
) -> Result<usize> {
    let statement = transaction.prepare(INSERT_CZSC_STRUCTURE_SQL).await?;
    let mut rows = rows.iter().collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        left.trade_date
            .cmp(&right.trade_date)
            .then_with(|| left.ts_code.cmp(&right.ts_code))
    });
    let groups = count_trade_date_groups(&rows, |row| row.trade_date);
    let started = Instant::now();
    eprintln!(
        "[calc] write stock_daily_czsc_structure start rows={} trade_date_groups={}",
        rows.len(),
        groups
    );
    let mut completed_groups = 0;
    let mut group_rows = 0;
    for (index, row) in rows.iter().enumerate() {
        transaction
            .execute(
                &statement,
                &[
                    &row.ts_code,
                    &row.trade_date,
                    &row.source_bar_valid,
                    &row.status,
                    &row.available_finished_bi_count,
                    &row.recent_bi_count,
                    &row.last_bi_direction,
                    &row.last_bi_start_date,
                    &row.last_bi_end_date,
                    &row.last_bi_high,
                    &row.last_bi_low,
                    &row.last_bi_mid,
                    &row.latest_zs_start_date,
                    &row.latest_zs_end_date,
                    &row.latest_zs_bi_count,
                    &row.latest_zs_zg,
                    &row.latest_zs_zd,
                    &row.latest_zs_zz,
                    &row.recent_peak_date,
                    &row.recent_peak_high,
                    &row.post_peak_first_bear_date,
                    &row.post_peak_first_bear_open,
                    &row.post_peak_first_bear_high,
                    &row.post_peak_first_bear_low,
                    &row.post_peak_first_bear_close,
                    &row.post_peak_first_bear_missing,
                    &CZSC_STRUCTURE_CALC_VERSION,
                ],
            )
            .await?;
        group_rows += 1;
        if rows
            .get(index + 1)
            .is_none_or(|next| next.trade_date != row.trade_date)
        {
            completed_groups += 1;
            log_calc_date_group_progress(CalcDateGroupProgress {
                table: "stock_daily_czsc_structure",
                date_label: "trade_date",
                date: row.trade_date,
                period_type: None,
                group_rows,
                completed_groups,
                total_groups: groups,
                written_rows: index + 1,
                total_rows: rows.len(),
                started,
            });
            group_rows = 0;
        }
    }
    eprintln!(
        "[calc] write stock_daily_czsc_structure done rows={} elapsed={:.1}s",
        rows.len(),
        started.elapsed().as_secs_f64()
    );
    Ok(rows.len())
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

fn pending_period_rows<'a>(
    rows: &'a [PeriodStateRow],
    existing_period_state_keys: &HashSet<(String, String, NaiveDate)>,
) -> Vec<&'a PeriodStateRow> {
    rows.iter()
        .filter(|row| !has_period_state(existing_period_state_keys, row))
        .collect()
}

fn pending_non_zero_dif_period_rows<'a>(
    rows: &'a [PeriodStateRow],
    existing_period_state_keys: &HashSet<(String, String, NaiveDate)>,
) -> Vec<&'a PeriodStateRow> {
    rows.iter()
        .filter(|row| !has_period_state(existing_period_state_keys, row))
        .filter(|row| row.dif != Some(0.0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_period_rows_are_not_filtered_by_stock_period_keys() {
        let date = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        let index_row = period_row("000001.SH", date);
        let existing_stock_keys =
            HashSet::from([("000001.SH".to_string(), "daily".to_string(), date)]);
        let existing_index_keys = HashSet::new();

        assert_eq!(
            pending_period_rows(&[index_row], &existing_stock_keys).len(),
            0
        );
        let index_row = period_row("000001.SH", date);
        assert_eq!(
            pending_period_rows(&[index_row], &existing_index_keys).len(),
            1
        );
    }

    #[test]
    fn index_period_rows_skip_zero_dif_states() {
        let date = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        let zero_dif_row = period_row("000001.SH", date);
        let existing_index_keys = HashSet::new();

        assert_eq!(
            pending_non_zero_dif_period_rows(&[zero_dif_row], &existing_index_keys).len(),
            0
        );
    }

    #[test]
    fn czsc_upsert_updates_every_non_key_snapshot_column() {
        assert!(INSERT_CZSC_STRUCTURE_SQL
            .contains(r#"on conflict ("ts_code", "trade_date", "calc_version") do update set"#));
        for column in [
            "source_bar_valid",
            "status",
            "available_finished_bi_count",
            "recent_bi_count",
            "last_bi_direction",
            "last_bi_start_date",
            "last_bi_end_date",
            "last_bi_high",
            "last_bi_low",
            "last_bi_mid",
            "latest_zs_start_date",
            "latest_zs_end_date",
            "latest_zs_bi_count",
            "latest_zs_zg",
            "latest_zs_zd",
            "latest_zs_zz",
            "recent_peak_date",
            "recent_peak_high",
            "post_peak_first_bear_date",
            "post_peak_first_bear_open",
            "post_peak_first_bear_high",
            "post_peak_first_bear_low",
            "post_peak_first_bear_close",
            "post_peak_first_bear_missing",
        ] {
            assert!(
                INSERT_CZSC_STRUCTURE_SQL.contains(&format!(r#""{column}" = excluded."{column}""#)),
                "missing update for {column}"
            );
        }
        assert!(INSERT_CZSC_STRUCTURE_SQL.contains("$27"));
    }

    fn period_row(ts_code: &str, period_end_date: NaiveDate) -> PeriodStateRow {
        PeriodStateRow {
            ts_code: ts_code.to_string(),
            period_type: "daily".to_string(),
            period_start_date: period_end_date,
            period_end_date,
            period_close_qfq: Some(1.0),
            ema12: Some(1.0),
            ema26: Some(1.0),
            dif: Some(0.0),
            dea: Some(0.0),
            hist_x2: Some(0.0),
            period_count: Some(1),
        }
    }
}
