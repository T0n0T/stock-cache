use std::collections::{BTreeMap, HashSet};

use anyhow::{anyhow, Result};
use chrono::{Datelike, Duration, NaiveDate};
use serde::Serialize;
use serde_json::json;
use tokio_postgres::NoTls;

use crate::{config::Config, source_writer::parse_date_text, write::DateWindow};

const MACD_CALC_VERSION: &str = "macd_qfq_12_26_9_v1";
const ROLLING_CALC_VERSION: &str = "rolling_qfq_v1";
const LEFT_PEAK_CALC_VERSION: &str = "left_peak_qfq_v1";

const ALPHA12: f64 = 2.0 / (12.0 + 1.0);
const ALPHA26: f64 = 2.0 / (26.0 + 1.0);
const ALPHA9: f64 = 2.0 / (9.0 + 1.0);

const SWING_LOOKBACK: usize = 2;
const BREAKOUT_LOOKBACK: usize = 5;
const HIGH_CYCLE_DAYS: usize = 360;
const MIN_PULLBACK_PCT: f64 = 5.0;

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

pub async fn run_calc(window: &DateWindow, config: &Config) -> Result<CalcSummary> {
    let (mut client, connection) = tokio_postgres::connect(&config.postgres_dsn, NoTls).await?;
    let connection_task = tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("postgres connection error: {error}");
        }
    });

    let target_dates = resolve_calc_dates(&client, window).await?;
    let result = if target_dates.is_empty() {
        Ok(empty_summary())
    } else {
        let max_target_date = *target_dates.last().expect("checked non-empty");
        let source_rows = load_source_rows(&client, max_target_date).await?;
        let distinct_dates = distinct_trade_dates(&source_rows);
        let computed = compute_calc_rows(&source_rows, &target_dates);
        let row_counts = write_calc_rows(&mut client, &computed).await?;
        Ok(summary(
            source_rows.len(),
            &target_dates,
            &distinct_dates,
            row_counts,
        ))
    };

    connection_task.abort();
    result
}

pub fn summary_json(summary: &CalcSummary) -> Result<String> {
    Ok(serde_json::to_string_pretty(&json!(summary))?)
}

async fn resolve_calc_dates(
    client: &tokio_postgres::Client,
    window: &DateWindow,
) -> Result<Vec<NaiveDate>> {
    match window {
        DateWindow::Explicit {
            start_date,
            end_date,
        } => {
            let start = parse_date_text(start_date)?;
            let end = parse_date_text(end_date)?;
            let rows = client
                .query(
                    r#"
                    select distinct "trade_date"
                    from "stock_stk_factor_pro"
                    where "trade_date" between $1 and $2
                    order by "trade_date"
                    "#,
                    &[&start, &end],
                )
                .await?;
            Ok(rows.into_iter().map(|row| row.get(0)).collect())
        }
        DateWindow::LookbackTradingDays(days) | DateWindow::DefaultLookbackTradingDays(days) => {
            let limit = i64::from(*days);
            let rows = client
                .query(
                    r#"
                    select distinct "trade_date"
                    from "stock_stk_factor_pro"
                    order by "trade_date" desc
                    limit $1
                    "#,
                    &[&limit],
                )
                .await?;
            let mut dates = rows.into_iter().map(|row| row.get(0)).collect::<Vec<_>>();
            dates.reverse();
            Ok(dates)
        }
        DateWindow::RefreshThsOnly => Err(anyhow!(
            "calc execution requires a stock_stk_factor_pro date window"
        )),
    }
}

async fn load_source_rows(
    client: &tokio_postgres::Client,
    max_trade_date: NaiveDate,
) -> Result<Vec<SourceBar>> {
    let rows = client
        .query(
            r#"
            select "ts_code", "trade_date", "open_qfq", "high_qfq", "low_qfq", "close_qfq", "vol", "turnover_rate"
            from "stock_stk_factor_pro"
            where "trade_date" <= $1
            order by "ts_code", "trade_date"
            "#,
            &[&max_trade_date],
        )
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| SourceBar {
            ts_code: row.get("ts_code"),
            trade_date: row.get("trade_date"),
            open_qfq: row.get("open_qfq"),
            high_qfq: row.get("high_qfq"),
            low_qfq: row.get("low_qfq"),
            close_qfq: row.get("close_qfq"),
            vol: row.get("vol"),
            turnover_rate: row.get("turnover_rate"),
        })
        .collect())
}

fn distinct_trade_dates(source_rows: &[SourceBar]) -> Vec<NaiveDate> {
    let mut dates = source_rows
        .iter()
        .map(|row| row.trade_date)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    dates.sort_unstable();
    dates
}

fn compute_calc_rows(source_rows: &[SourceBar], target_dates: &[NaiveDate]) -> ComputedRows {
    let target_dates = target_dates.iter().copied().collect::<HashSet<_>>();
    let mut computed = ComputedRows::default();
    let mut offset = 0;
    while offset < source_rows.len() {
        let ts_code = &source_rows[offset].ts_code;
        let start = offset;
        while offset < source_rows.len() && source_rows[offset].ts_code == *ts_code {
            offset += 1;
        }
        compute_stock_rows(&source_rows[start..offset], &target_dates, &mut computed);
    }
    computed
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

fn finalize_period(
    ts_code: &str,
    period_type: &str,
    period: &PeriodBuilder,
    macd: &mut MacdAccumulator,
    deas: &mut Vec<f64>,
    rows: &mut Vec<PeriodStateRow>,
) {
    if let Some(close) = period.close_qfq {
        let state = macd.push(close);
        deas.push(state.dea);
        rows.push(PeriodStateRow::from_state(
            ts_code,
            period_type,
            period.start_date,
            period.end_date,
            close,
            state,
        ));
    }
}

fn compute_asof_row(
    bar: &SourceBar,
    daily_state: Option<MacdState>,
    daily_deas: &[f64],
    weekly_macd: &MacdAccumulator,
    weekly_deas: &[f64],
    monthly_macd: &MacdAccumulator,
    monthly_deas: &[f64],
) -> AsofRow {
    let weekly_state = bar.close_qfq.map(|close| weekly_macd.preview_next(close));
    let monthly_state = bar.close_qfq.map(|close| monthly_macd.preview_next(close));
    AsofRow {
        ts_code: bar.ts_code.clone(),
        trade_date: bar.trade_date,
        daily_dif_asof: daily_state.map(|state| state.dif),
        daily_dea_asof: daily_state.map(|state| state.dea),
        daily_hist_x2_asof: daily_state.map(|state| state.hist_x2),
        daily_dea_pctile_asof: daily_state
            .and_then(|state| percentile_inclusive(daily_deas, state.dea)),
        daily_period_count: daily_state.map(|state| state.period_count),
        weekly_dif_asof: weekly_state.map(|state| state.dif),
        weekly_dea_asof: weekly_state.map(|state| state.dea),
        weekly_hist_x2_asof: weekly_state.map(|state| state.hist_x2),
        weekly_dea_pctile_asof: weekly_state
            .map(|state| percentile_with_current(weekly_deas, state.dea)),
        weekly_period_count: weekly_state.map(|state| state.period_count),
        monthly_dif_asof: monthly_state.map(|state| state.dif),
        monthly_dea_asof: monthly_state.map(|state| state.dea),
        monthly_hist_x2_asof: monthly_state.map(|state| state.hist_x2),
        monthly_dea_pctile_asof: monthly_state
            .map(|state| percentile_with_current(monthly_deas, state.dea)),
        monthly_period_count: monthly_state.map(|state| state.period_count),
    }
}

fn compute_rolling_row(bars: &[SourceBar], idx: usize) -> RollingRow {
    let bar = &bars[idx];
    let close = bar.close_qfq;
    let high_20 = rolling_extreme(bars, idx, 20, PriceField::High, Extreme::Max);
    let low_20 = rolling_extreme(bars, idx, 20, PriceField::Low, Extreme::Min);
    let high_90 = rolling_extreme(bars, idx, 90, PriceField::High, Extreme::Max);
    let low_90 = rolling_extreme(bars, idx, 90, PriceField::Low, Extreme::Min);
    let high_120 = rolling_extreme(bars, idx, 120, PriceField::High, Extreme::Max);
    let low_120 = rolling_extreme(bars, idx, 120, PriceField::Low, Extreme::Min);
    let max_close_20 = rolling_extreme(bars, idx, 20, PriceField::Close, Extreme::Max);
    let volume_ma5 = rolling_average(bars, idx, 5, PriceField::Vol);
    let volume_ma20 = rolling_average(bars, idx, 20, PriceField::Vol);
    let turnover_rate_ma5 = rolling_average(bars, idx, 5, PriceField::TurnoverRate);
    RollingRow {
        ts_code: bar.ts_code.clone(),
        trade_date: bar.trade_date,
        ma25_qfq: rolling_average(bars, idx, 25, PriceField::Close),
        ma144_qfq: rolling_average(bars, idx, 144, PriceField::Close),
        ma220_qfq: rolling_average(bars, idx, 220, PriceField::Close),
        high_20_qfq: high_20,
        high_90_qfq: high_90,
        low_90_qfq: low_90,
        high_120_qfq: high_120,
        low_120_qfq: low_120,
        close_to_20d_max_close_pct: ratio_minus_one(close, max_close_20),
        position_90d: position(close, low_90, high_90),
        position_120d: position(close, low_120, high_120),
        volume_ma5,
        volume_ma20,
        volume_to_ma5_ratio: ratio(bar.vol, volume_ma5),
        volume_to_ma20_ratio: ratio(bar.vol, volume_ma20),
        volume_ma5_to_ma20_ratio: ratio(volume_ma5, volume_ma20),
        turnover_rate_ma5,
        turnover_to_ma5_ratio: ratio(bar.turnover_rate, turnover_rate_ma5),
        range_compression_20d: range_compression(high_20, low_20, close),
        range_compression_40d: range_compression(
            rolling_extreme(bars, idx, 40, PriceField::High, Extreme::Max),
            rolling_extreme(bars, idx, 40, PriceField::Low, Extreme::Min),
            close,
        ),
    }
}

fn compute_left_peak_row(bars: &[SourceBar], idx: usize) -> LeftPeakRow {
    let frame = bars[..=idx]
        .iter()
        .filter(|bar| {
            bar.open_qfq.is_some()
                && bar.high_qfq.is_some()
                && bar.low_qfq.is_some()
                && bar.close_qfq.is_some()
        })
        .cloned()
        .collect::<Vec<_>>();
    let target = &bars[idx];
    let pick_close = target.close_qfq;
    if frame.len() < 20.max(BREAKOUT_LOOKBACK + SWING_LOOKBACK * 2 + 1) {
        return LeftPeakRow::empty(target, pick_close, "insufficient_history");
    }

    let recent_start = frame.len().saturating_sub(BREAKOUT_LOOKBACK);
    let prior_end = recent_start;
    if prior_end == 0 {
        return LeftPeakRow::empty(target, pick_close, "insufficient_history");
    }
    let peaks = swing_highs(&frame[..prior_end]);
    if peaks.is_empty() {
        return LeftPeakRow::empty(target, pick_close, "no_swing_high");
    }
    let cycle_start = frame.len().saturating_sub(HIGH_CYCLE_DAYS);
    let left_peak_idx =
        peaks
            .into_iter()
            .filter(|idx| *idx >= cycle_start)
            .max_by(|left, right| {
                high(&frame[*left])
                    .partial_cmp(&high(&frame[*right]))
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| left.cmp(right))
            });
    let Some(left_peak_idx) = left_peak_idx else {
        return LeftPeakRow::empty(target, pick_close, "no_cycle_peak");
    };
    let left_peak = &frame[left_peak_idx];
    let left_peak_high = high(left_peak);
    if left_peak_high <= 0.0 {
        return LeftPeakRow::with_peak(
            target,
            pick_close,
            &frame,
            left_peak,
            None,
            "no_close_breakout",
        );
    }

    let breakout_idx = (recent_start..frame.len()).find(|idx| close(&frame[*idx]) > left_peak_high);
    let Some(breakout_idx) = breakout_idx else {
        return LeftPeakRow::with_peak(
            target,
            pick_close,
            &frame,
            left_peak,
            None,
            "no_close_breakout",
        );
    };
    let breakout = &frame[breakout_idx];
    let body_window_start = breakout_idx.saturating_sub(BREAKOUT_LOOKBACK - 1);
    let body_above = frame[body_window_start..=breakout_idx]
        .iter()
        .any(|bar| open(bar) > left_peak_high && close(bar) > left_peak_high);
    if !body_above {
        return LeftPeakRow::with_peak_and_breakout(
            target,
            pick_close,
            &frame,
            left_peak,
            breakout,
            Some(false),
            "no_body_breakout",
        );
    }

    let min_low = frame[left_peak_idx + 1..=breakout_idx]
        .iter()
        .map(low)
        .reduce(f64::min);
    let Some(min_low) = min_low else {
        return LeftPeakRow::with_peak_and_breakout(
            target,
            pick_close,
            &frame,
            left_peak,
            breakout,
            Some(true),
            "pullback_too_small",
        );
    };
    let pullback_pct = (left_peak_high - min_low) / left_peak_high * 100.0;
    if pullback_pct < MIN_PULLBACK_PCT {
        return LeftPeakRow::with_peak_and_breakout(
            target,
            pick_close,
            &frame,
            left_peak,
            breakout,
            Some(true),
            "pullback_too_small",
        );
    }

    LeftPeakRow::with_peak_and_breakout(
        target,
        pick_close,
        &frame,
        left_peak,
        breakout,
        Some(true),
        "valid",
    )
}

fn swing_highs(frame: &[SourceBar]) -> Vec<usize> {
    if frame.len() < SWING_LOOKBACK * 2 + 1 {
        return Vec::new();
    }
    let mut peaks = Vec::new();
    for idx in SWING_LOOKBACK..frame.len() - SWING_LOOKBACK {
        let value = high(&frame[idx]);
        let left_max = frame[idx - SWING_LOOKBACK..idx]
            .iter()
            .map(high)
            .fold(f64::NEG_INFINITY, f64::max);
        let right_max = frame[idx + 1..=idx + SWING_LOOKBACK]
            .iter()
            .map(high)
            .fold(f64::NEG_INFINITY, f64::max);
        if value > left_max && value >= right_max {
            peaks.push(idx);
        }
    }
    peaks
}

async fn write_calc_rows(
    client: &mut tokio_postgres::Client,
    computed: &ComputedRows,
) -> Result<BTreeMap<String, usize>> {
    let transaction = client.transaction().await?;
    for row in &computed.period_states {
        transaction
            .execute(
                INSERT_PERIOD_STATE_SQL,
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
    }
    for row in &computed.asof_rows {
        transaction
            .execute(
                INSERT_ASOF_SQL,
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
    }
    for row in &computed.rolling_rows {
        transaction
            .execute(
                INSERT_ROLLING_SQL,
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
    }
    for row in &computed.left_peak_rows {
        transaction
            .execute(
                INSERT_LEFT_PEAK_SQL,
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
    }
    transaction.commit().await?;

    Ok(BTreeMap::from([
        (
            "stock_period_indicator_state".to_string(),
            computed.period_states.len(),
        ),
        (
            "stock_daily_asof_indicators".to_string(),
            computed.asof_rows.len(),
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

fn summary(
    source_row_count: usize,
    target_dates: &[NaiveDate],
    distinct_dates: &[NaiveDate],
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
    summary(0, &[], &[], BTreeMap::new())
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

#[derive(Debug, Clone)]
struct SourceBar {
    ts_code: String,
    trade_date: NaiveDate,
    open_qfq: Option<f64>,
    high_qfq: Option<f64>,
    low_qfq: Option<f64>,
    close_qfq: Option<f64>,
    vol: Option<f64>,
    turnover_rate: Option<f64>,
}

#[derive(Debug, Default)]
struct ComputedRows {
    period_states: Vec<PeriodStateRow>,
    asof_rows: Vec<AsofRow>,
    rolling_rows: Vec<RollingRow>,
    left_peak_rows: Vec<LeftPeakRow>,
}

#[derive(Debug)]
struct PeriodStateRow {
    ts_code: String,
    period_type: String,
    period_start_date: NaiveDate,
    period_end_date: NaiveDate,
    period_close_qfq: Option<f64>,
    ema12: Option<f64>,
    ema26: Option<f64>,
    dif: Option<f64>,
    dea: Option<f64>,
    hist_x2: Option<f64>,
    period_count: Option<i32>,
}

impl PeriodStateRow {
    fn from_state(
        ts_code: &str,
        period_type: &str,
        period_start_date: NaiveDate,
        period_end_date: NaiveDate,
        period_close_qfq: f64,
        state: MacdState,
    ) -> Self {
        Self {
            ts_code: ts_code.to_string(),
            period_type: period_type.to_string(),
            period_start_date,
            period_end_date,
            period_close_qfq: Some(period_close_qfq),
            ema12: Some(state.ema12),
            ema26: Some(state.ema26),
            dif: Some(state.dif),
            dea: Some(state.dea),
            hist_x2: Some(state.hist_x2),
            period_count: Some(state.period_count),
        }
    }
}

#[derive(Debug)]
struct AsofRow {
    ts_code: String,
    trade_date: NaiveDate,
    daily_dif_asof: Option<f64>,
    daily_dea_asof: Option<f64>,
    daily_hist_x2_asof: Option<f64>,
    daily_dea_pctile_asof: Option<f64>,
    daily_period_count: Option<i32>,
    weekly_dif_asof: Option<f64>,
    weekly_dea_asof: Option<f64>,
    weekly_hist_x2_asof: Option<f64>,
    weekly_dea_pctile_asof: Option<f64>,
    weekly_period_count: Option<i32>,
    monthly_dif_asof: Option<f64>,
    monthly_dea_asof: Option<f64>,
    monthly_hist_x2_asof: Option<f64>,
    monthly_dea_pctile_asof: Option<f64>,
    monthly_period_count: Option<i32>,
}

#[derive(Debug)]
struct RollingRow {
    ts_code: String,
    trade_date: NaiveDate,
    ma25_qfq: Option<f64>,
    ma144_qfq: Option<f64>,
    ma220_qfq: Option<f64>,
    high_20_qfq: Option<f64>,
    high_90_qfq: Option<f64>,
    low_90_qfq: Option<f64>,
    high_120_qfq: Option<f64>,
    low_120_qfq: Option<f64>,
    close_to_20d_max_close_pct: Option<f64>,
    position_90d: Option<f64>,
    position_120d: Option<f64>,
    volume_ma5: Option<f64>,
    volume_ma20: Option<f64>,
    volume_to_ma5_ratio: Option<f64>,
    volume_to_ma20_ratio: Option<f64>,
    volume_ma5_to_ma20_ratio: Option<f64>,
    turnover_rate_ma5: Option<f64>,
    turnover_to_ma5_ratio: Option<f64>,
    range_compression_20d: Option<f64>,
    range_compression_40d: Option<f64>,
}

#[derive(Debug)]
struct LeftPeakRow {
    ts_code: String,
    trade_date: NaiveDate,
    left_peak_date: Option<NaiveDate>,
    left_peak_high: Option<f64>,
    breakout_date: Option<NaiveDate>,
    breakout_close: Option<f64>,
    breakout_body_above_left_peak: Option<bool>,
    first_bear_date: Option<NaiveDate>,
    first_bear_open: Option<f64>,
    first_bear_missing: Option<bool>,
    pick_close: Option<f64>,
    b_div_a: Option<f64>,
    abs_ba_minus_1: Option<f64>,
    a_lt_b: Option<bool>,
    is_valid: Option<bool>,
    status: String,
}

impl LeftPeakRow {
    fn empty(target: &SourceBar, pick_close: Option<f64>, status: &str) -> Self {
        Self {
            ts_code: target.ts_code.clone(),
            trade_date: target.trade_date,
            left_peak_date: None,
            left_peak_high: None,
            breakout_date: None,
            breakout_close: None,
            breakout_body_above_left_peak: None,
            first_bear_date: None,
            first_bear_open: None,
            first_bear_missing: None,
            pick_close,
            b_div_a: None,
            abs_ba_minus_1: None,
            a_lt_b: None,
            is_valid: Some(false),
            status: status.to_string(),
        }
    }

    fn with_peak(
        target: &SourceBar,
        pick_close: Option<f64>,
        frame: &[SourceBar],
        left_peak: &SourceBar,
        breakout: Option<&SourceBar>,
        status: &str,
    ) -> Self {
        let (first_bear_date, first_bear_open, first_bear_missing) =
            first_bear_after(frame, left_peak.trade_date, target.trade_date);
        let anchor = first_bear_open.and_then(|anchor| derive_anchor_values(pick_close, anchor));
        Self {
            ts_code: target.ts_code.clone(),
            trade_date: target.trade_date,
            left_peak_date: Some(left_peak.trade_date),
            left_peak_high: left_peak.high_qfq,
            breakout_date: breakout.map(|bar| bar.trade_date),
            breakout_close: breakout.and_then(|bar| bar.close_qfq),
            breakout_body_above_left_peak: None,
            first_bear_date,
            first_bear_open,
            first_bear_missing: Some(first_bear_missing),
            pick_close,
            b_div_a: anchor.map(|values| values.0),
            abs_ba_minus_1: anchor.map(|values| values.1),
            a_lt_b: anchor.map(|values| values.2),
            is_valid: Some(false),
            status: status.to_string(),
        }
    }

    fn with_peak_and_breakout(
        target: &SourceBar,
        pick_close: Option<f64>,
        frame: &[SourceBar],
        left_peak: &SourceBar,
        breakout: &SourceBar,
        body_above: Option<bool>,
        status: &str,
    ) -> Self {
        let mut row = Self::with_peak(target, pick_close, frame, left_peak, Some(breakout), status);
        row.breakout_body_above_left_peak = body_above;
        row.is_valid = Some(status == "valid");
        row
    }
}

fn first_bear_after(
    frame: &[SourceBar],
    left_peak_date: NaiveDate,
    target_date: NaiveDate,
) -> (Option<NaiveDate>, Option<f64>, bool) {
    let first_bear = frame.iter().find(|bar| {
        bar.trade_date > left_peak_date && bar.trade_date <= target_date && close(bar) < open(bar)
    });
    match first_bear {
        Some(bar) => (Some(bar.trade_date), bar.open_qfq, false),
        None => (None, None, true),
    }
}

fn derive_anchor_values(pick_close: Option<f64>, anchor_open: f64) -> Option<(f64, f64, bool)> {
    let pick_close = pick_close?;
    if anchor_open == 0.0 {
        return None;
    }
    let b_div_a = pick_close / anchor_open;
    Some((b_div_a, (b_div_a - 1.0).abs(), anchor_open < pick_close))
}

#[derive(Debug, Default, Clone, Copy)]
struct MacdAccumulator {
    state: Option<MacdState>,
}

impl MacdAccumulator {
    fn push(&mut self, close: f64) -> MacdState {
        let next = self.preview_next(close);
        self.state = Some(next);
        next
    }

    fn preview_next(&self, close: f64) -> MacdState {
        match self.state {
            None => MacdState {
                ema12: close,
                ema26: close,
                dif: 0.0,
                dea: 0.0,
                hist_x2: 0.0,
                period_count: 1,
            },
            Some(previous) => {
                let ema12 = ALPHA12 * close + (1.0 - ALPHA12) * previous.ema12;
                let ema26 = ALPHA26 * close + (1.0 - ALPHA26) * previous.ema26;
                let dif = ema12 - ema26;
                let dea = ALPHA9 * dif + (1.0 - ALPHA9) * previous.dea;
                MacdState {
                    ema12,
                    ema26,
                    dif,
                    dea,
                    hist_x2: 2.0 * (dif - dea),
                    period_count: previous.period_count + 1,
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct MacdState {
    ema12: f64,
    ema26: f64,
    dif: f64,
    dea: f64,
    hist_x2: f64,
    period_count: i32,
}

#[derive(Debug, Clone, Copy)]
enum PeriodKind {
    Weekly,
    Monthly,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum PeriodKey {
    Weekly { year: i32, week: u32 },
    Monthly { year: i32, month: u32 },
}

#[derive(Debug)]
struct PeriodBuilder {
    kind: PeriodKind,
    key: PeriodKey,
    start_date: NaiveDate,
    end_date: NaiveDate,
    close_qfq: Option<f64>,
}

impl PeriodBuilder {
    fn new(kind: PeriodKind, date: NaiveDate) -> Self {
        Self {
            kind,
            key: period_key(kind, date),
            start_date: period_start_date(kind, date),
            end_date: date,
            close_qfq: None,
        }
    }

    fn contains(&self, date: NaiveDate) -> bool {
        self.key == period_key(self.kind, date)
    }

    fn observe(&mut self, bar: &SourceBar) {
        self.end_date = bar.trade_date;
        if let Some(close) = bar.close_qfq {
            self.close_qfq = Some(close);
        }
    }
}

fn period_key(kind: PeriodKind, date: NaiveDate) -> PeriodKey {
    match kind {
        PeriodKind::Weekly => {
            let iso = date.iso_week();
            PeriodKey::Weekly {
                year: iso.year(),
                week: iso.week(),
            }
        }
        PeriodKind::Monthly => PeriodKey::Monthly {
            year: date.year(),
            month: date.month(),
        },
    }
}

fn period_start_date(kind: PeriodKind, date: NaiveDate) -> NaiveDate {
    match kind {
        PeriodKind::Weekly => {
            date - Duration::days(i64::from(date.weekday().num_days_from_monday()))
        }
        PeriodKind::Monthly => {
            NaiveDate::from_ymd_opt(date.year(), date.month(), 1).expect("valid month start")
        }
    }
}

#[derive(Clone, Copy)]
enum PriceField {
    High,
    Low,
    Close,
    Vol,
    TurnoverRate,
}

#[derive(Clone, Copy)]
enum Extreme {
    Max,
    Min,
}

fn rolling_average(
    bars: &[SourceBar],
    idx: usize,
    window: usize,
    field: PriceField,
) -> Option<f64> {
    let values = rolling_values(bars, idx, window, field)?;
    Some(values.iter().sum::<f64>() / values.len() as f64)
}

fn rolling_extreme(
    bars: &[SourceBar],
    idx: usize,
    window: usize,
    field: PriceField,
    extreme: Extreme,
) -> Option<f64> {
    let values = rolling_values(bars, idx, window, field)?;
    match extreme {
        Extreme::Max => values.into_iter().reduce(f64::max),
        Extreme::Min => values.into_iter().reduce(f64::min),
    }
}

fn rolling_values(
    bars: &[SourceBar],
    idx: usize,
    window: usize,
    field: PriceField,
) -> Option<Vec<f64>> {
    if idx + 1 < window {
        return None;
    }
    bars[idx + 1 - window..=idx]
        .iter()
        .map(|bar| value(bar, field))
        .collect()
}

fn value(bar: &SourceBar, field: PriceField) -> Option<f64> {
    match field {
        PriceField::High => bar.high_qfq,
        PriceField::Low => bar.low_qfq,
        PriceField::Close => bar.close_qfq,
        PriceField::Vol => bar.vol,
        PriceField::TurnoverRate => bar.turnover_rate,
    }
}

fn ratio(numerator: Option<f64>, denominator: Option<f64>) -> Option<f64> {
    let denominator = denominator?;
    if denominator == 0.0 {
        return None;
    }
    Some(numerator? / denominator)
}

fn ratio_minus_one(numerator: Option<f64>, denominator: Option<f64>) -> Option<f64> {
    ratio(numerator, denominator).map(|value| value - 1.0)
}

fn position(close: Option<f64>, low: Option<f64>, high: Option<f64>) -> Option<f64> {
    let low = low?;
    let high = high?;
    let denominator = high - low;
    if denominator == 0.0 {
        return None;
    }
    Some((close? - low) / denominator)
}

fn range_compression(high: Option<f64>, low: Option<f64>, close: Option<f64>) -> Option<f64> {
    let close = close?;
    if close == 0.0 {
        return None;
    }
    Some((high? - low?) / close)
}

fn percentile_inclusive(values: &[f64], current: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    Some(values.iter().filter(|value| **value <= current).count() as f64 / values.len() as f64)
}

fn percentile_with_current(previous: &[f64], current: f64) -> f64 {
    let less_or_equal = previous.iter().filter(|value| **value <= current).count() + 1;
    less_or_equal as f64 / (previous.len() + 1) as f64
}

fn open(bar: &SourceBar) -> f64 {
    bar.open_qfq.expect("left peak frame filters open_qfq")
}

fn high(bar: &SourceBar) -> f64 {
    bar.high_qfq.expect("left peak frame filters high_qfq")
}

fn low(bar: &SourceBar) -> f64 {
    bar.low_qfq.expect("left peak frame filters low_qfq")
}

fn close(bar: &SourceBar) -> f64 {
    bar.close_qfq.expect("left peak frame filters close_qfq")
}

fn close_or_nan(close: Option<f64>) -> f64 {
    close.expect("MACD state only exists with close_qfq")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macd_seeds_first_close_and_recurs() {
        let mut macd = MacdAccumulator::default();
        let first = macd.push(10.0);
        assert_eq!(first.ema12, 10.0);
        assert_eq!(first.ema26, 10.0);
        assert_eq!(first.dif, 0.0);
        assert_eq!(first.dea, 0.0);

        let second = macd.push(11.0);
        assert!(second.dif > 0.0);
        assert_eq!(second.period_count, 2);
        assert!((second.hist_x2 - 2.0 * (second.dif - second.dea)).abs() < 1e-12);
    }

    #[test]
    fn rolling_values_require_complete_window() {
        let bars = (1..=25)
            .map(|day| bar("000001.SZ", 2025, 1, day, day as f64))
            .collect::<Vec<_>>();
        assert_eq!(rolling_average(&bars, 23, 25, PriceField::Close), None);
        assert_eq!(
            rolling_average(&bars, 24, 25, PriceField::Close),
            Some(13.0)
        );
    }

    #[test]
    fn period_start_dates_use_natural_week_and_month_bounds() {
        let date = NaiveDate::from_ymd_opt(2025, 6, 25).unwrap();
        assert_eq!(
            period_start_date(PeriodKind::Weekly, date),
            NaiveDate::from_ymd_opt(2025, 6, 23).unwrap()
        );
        assert_eq!(
            period_start_date(PeriodKind::Monthly, date),
            NaiveDate::from_ymd_opt(2025, 6, 1).unwrap()
        );
    }

    #[test]
    fn computes_left_peak_valid_breakout_with_anchor() {
        let mut bars = Vec::new();
        for day in 1..=25 {
            let (open, high, low, close) = match day {
                5 => (9.8, 12.0, 9.5, 11.0),
                6 => (10.8, 11.0, 10.0, 10.2),
                20 => (11.0, 11.5, 10.5, 11.2),
                21 => (11.4, 11.8, 10.8, 11.5),
                22 => (12.1, 12.8, 11.9, 12.4),
                23 => (12.2, 12.5, 11.8, 12.3),
                24 => (12.0, 12.6, 11.7, 12.4),
                25 => (12.2, 12.7, 12.0, 12.5),
                _ => (9.5, 10.0, 9.0, 9.6),
            };
            bars.push(SourceBar {
                ts_code: "000001.SZ".to_string(),
                trade_date: NaiveDate::from_ymd_opt(2025, 1, day).unwrap(),
                open_qfq: Some(open),
                high_qfq: Some(high),
                low_qfq: Some(low),
                close_qfq: Some(close),
                vol: Some(1.0),
                turnover_rate: Some(1.0),
            });
        }

        let row = compute_left_peak_row(&bars, 24);
        assert_eq!(row.status, "valid");
        assert_eq!(
            row.left_peak_date,
            Some(NaiveDate::from_ymd_opt(2025, 1, 5).unwrap())
        );
        assert_eq!(
            row.first_bear_date,
            Some(NaiveDate::from_ymd_opt(2025, 1, 6).unwrap())
        );
        assert_eq!(row.first_bear_open, Some(10.8));
        assert_eq!(row.first_bear_missing, Some(false));
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
