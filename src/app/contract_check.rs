use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use serde::Serialize;
use tokio_postgres::{Client, NoTls};

use crate::{app::config::Config, cli::StockSelectContractArgs, source::writer::parse_date_text};

const MACD_CALC_VERSION: &str = "macd_qfq_12_26_9_v1";
const ROLLING_CALC_VERSION: &str = "rolling_qfq_v1";
const LEFT_PEAK_CALC_VERSION: &str = "left_peak_qfq_v1";
const KDJ_J_TOLERANCE: f64 = 0.0001;

pub async fn run_stock_select(args: StockSelectContractArgs, config: &Config) -> Result<()> {
    if args.lookback_trading_days <= 0 {
        return Err(anyhow!("--lookback-trading-days must be greater than 0"));
    }
    let (client, connection) = tokio_postgres::connect(&config.postgres_dsn, NoTls).await?;
    let connection_task = tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("postgres connection error: {error}");
        }
    });

    let result = async {
        let pick_date = match args.pick_date {
            Some(text) => parse_date_text(&text)?,
            None => latest_trade_date(&client).await?,
        };
        let report = build_report(
            &client,
            pick_date,
            args.lookback_trading_days,
            args.period_coverage_threshold,
        )
        .await?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        if report.blocking_failures.is_empty() {
            Ok(())
        } else {
            Err(anyhow!(
                "stock-select contract check failed: {} blocking failure(s)",
                report.blocking_failures.len()
            ))
        }
    }
    .await;

    connection_task.abort();
    result
}

async fn latest_trade_date(client: &Client) -> Result<NaiveDate> {
    let row = client
        .query_one(
            r#"select max("trade_date") from "stock_stk_factor_pro""#,
            &[],
        )
        .await?;
    row.get::<_, Option<NaiveDate>>(0)
        .ok_or_else(|| anyhow!("stock_stk_factor_pro has no trade_date"))
}

async fn build_report(
    client: &Client,
    pick_date: NaiveDate,
    lookback_trading_days: i64,
    period_coverage_threshold: f64,
) -> Result<ContractReport> {
    if !(0.0..=1.0).contains(&period_coverage_threshold) {
        return Err(anyhow!(
            "--period-coverage-threshold must be between 0.0 and 1.0"
        ));
    }
    let universe_rows = query_i64(
        client,
        r#"select count(*)::bigint from "stock_stk_factor_pro" where "trade_date" = $1"#,
        &[&pick_date],
    )
    .await?;

    let lookback = query_lookback(client, pick_date, lookback_trading_days).await?;
    let adj_factor = query_adj_factor(client, pick_date, lookback_trading_days).await?;
    let qfq_ohlc = query_qfq_ohlc(client, pick_date).await?;
    let kdj = query_kdj(client, pick_date).await?;
    let calc_tables = query_calc_tables(client, pick_date, universe_rows).await?;
    let period_state =
        query_period_state(client, pick_date, universe_rows, period_coverage_threshold).await?;
    let ths = query_ths(client, pick_date).await?;

    let mut blocking_failures = Vec::new();
    if universe_rows <= 0 {
        blocking_failures.push("stock_stk_factor_pro latest universe is empty".to_string());
    }
    if lookback.distinct_trade_days < lookback_trading_days {
        blocking_failures.push(format!(
            "stock_stk_factor_pro lookback has {} distinct trade days, expected {}",
            lookback.distinct_trade_days, lookback_trading_days
        ));
    }
    if adj_factor.rows <= 0 || adj_factor.rows != adj_factor.positive_finite {
        blocking_failures.push("adj_factor is not fully non-null, finite, and > 0".to_string());
    }
    for (field, coverage) in &qfq_ohlc.fields {
        if coverage.non_null != universe_rows {
            blocking_failures.push(format!("{field} is not fully non-null on pick_date"));
        }
    }
    if !kdj.has_k || !kdj.has_d {
        blocking_failures.push("kdj_k_qfq/kdj_d_qfq columns are required".to_string());
    }
    if !kdj.has_j_alias && !kdj.derivable_j {
        blocking_failures.push("KDJ J is neither available as kdj_j_qfq nor derivable".to_string());
    }
    for summary in calc_tables.values() {
        if summary.rows != universe_rows {
            blocking_failures.push(format!(
                "{} rows {} do not match universe {}",
                summary.table, summary.rows, universe_rows
            ));
        }
    }
    for (period, coverage) in [
        ("daily", &period_state.daily),
        ("weekly", &period_state.weekly),
        ("monthly", &period_state.monthly),
    ] {
        if coverage.coverage_ratio < period_coverage_threshold {
            blocking_failures.push(format!(
                "period state {period} coverage {:.6} is below threshold {:.6}",
                coverage.coverage_ratio, period_coverage_threshold
            ));
        }
    }
    if ths.index_ths_daily.rows <= 0
        || ths.index_daily_asof_indicators.rows <= 0
        || ths.index_moneyflow_cnt_ths.rows <= 0
        || ths.index_ths_member.rows <= 0
    {
        blocking_failures.push("one or more THS contract tables are empty".to_string());
    }

    Ok(ContractReport {
        status: if blocking_failures.is_empty() {
            "pass"
        } else {
            "fail"
        },
        contract: "stock-select-db-native",
        pick_date: date_text(pick_date),
        universe_rows,
        lookback,
        adj_factor,
        qfq_ohlc,
        kdj,
        calc_tables,
        period_state,
        ths,
        blocking_failures,
    })
}

async fn query_i64(
    client: &Client,
    sql: &str,
    params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
) -> Result<i64> {
    let row = client.query_one(sql, params).await?;
    Ok(row.get::<_, i64>(0))
}

async fn query_lookback(
    client: &Client,
    pick_date: NaiveDate,
    lookback_trading_days: i64,
) -> Result<LookbackSummary> {
    let row = client
        .query_one(
            r#"
            with dates as (
                select distinct "trade_date"
                from "stock_stk_factor_pro"
                where "trade_date" <= $1
                order by "trade_date" desc
                limit $2
            )
            select count(*)::bigint, min("trade_date"), max("trade_date")
            from dates
            "#,
            &[&pick_date, &lookback_trading_days],
        )
        .await?;
    Ok(LookbackSummary {
        expected_trade_days: lookback_trading_days,
        distinct_trade_days: row.get(0),
        start_date: optional_date_text(row.get(1)),
        end_date: optional_date_text(row.get(2)),
    })
}

async fn query_adj_factor(
    client: &Client,
    pick_date: NaiveDate,
    lookback_trading_days: i64,
) -> Result<AdjFactorSummary> {
    let row = client
        .query_one(
            r#"
            with dates as (
                select distinct "trade_date"
                from "stock_stk_factor_pro"
                where "trade_date" <= $1
                order by "trade_date" desc
                limit $2
            ), window_rows as (
                select s."adj_factor"
                from "stock_stk_factor_pro" s
                join dates d on d."trade_date" = s."trade_date"
            )
            select
                count(*)::bigint,
                count(*) filter (where "adj_factor" is not null)::bigint,
                count(*) filter (where "adj_factor" is not null and isfinite("adj_factor") and "adj_factor" > 0)::bigint,
                min("adj_factor"),
                max("adj_factor")
            from window_rows
            "#,
            &[&pick_date, &lookback_trading_days],
        )
        .await?;
    Ok(AdjFactorSummary {
        rows: row.get(0),
        non_null: row.get(1),
        positive_finite: row.get(2),
        min: row.get(3),
        max: row.get(4),
    })
}

async fn query_qfq_ohlc(client: &Client, pick_date: NaiveDate) -> Result<QfqOhlcSummary> {
    let row = client
        .query_one(
            r#"
            select
                count(*)::bigint,
                count(*) filter (where "open_qfq" is not null)::bigint,
                count(*) filter (where "high_qfq" is not null)::bigint,
                count(*) filter (where "low_qfq" is not null)::bigint,
                count(*) filter (where "close_qfq" is not null)::bigint
            from "stock_stk_factor_pro"
            where "trade_date" = $1
            "#,
            &[&pick_date],
        )
        .await?;
    let rows = row.get(0);
    let mut fields = BTreeMap::new();
    fields.insert(
        "open_qfq".to_string(),
        FieldCoverage {
            rows,
            non_null: row.get(1),
        },
    );
    fields.insert(
        "high_qfq".to_string(),
        FieldCoverage {
            rows,
            non_null: row.get(2),
        },
    );
    fields.insert(
        "low_qfq".to_string(),
        FieldCoverage {
            rows,
            non_null: row.get(3),
        },
    );
    fields.insert(
        "close_qfq".to_string(),
        FieldCoverage {
            rows,
            non_null: row.get(4),
        },
    );
    Ok(QfqOhlcSummary { fields })
}

async fn query_kdj(client: &Client, pick_date: NaiveDate) -> Result<KdjSummary> {
    let has_j_alias = has_column(client, "stock_stk_factor_pro", "kdj_j_qfq").await?;
    let row = client
        .query_one(
            r#"
            select
                count(*)::bigint,
                count(*) filter (where "kdj_k_qfq" is not null)::bigint,
                count(*) filter (where "kdj_d_qfq" is not null)::bigint,
                count(*) filter (where "kdj_qfq" is not null)::bigint,
                count(*) filter (
                    where "kdj_k_qfq" is not null
                      and "kdj_d_qfq" is not null
                      and "kdj_qfq" is not null
                      and abs("kdj_qfq" - (3 * "kdj_k_qfq" - 2 * "kdj_d_qfq")) <= $2
                )::bigint,
                max(abs("kdj_qfq" - (3 * "kdj_k_qfq" - 2 * "kdj_d_qfq"))) filter (
                    where "kdj_k_qfq" is not null and "kdj_d_qfq" is not null and "kdj_qfq" is not null
                )
            from "stock_stk_factor_pro"
            where "trade_date" = $1
            "#,
            &[&pick_date, &KDJ_J_TOLERANCE],
        )
        .await?;
    let rows = row.get(0);
    let k_non_null = row.get(1);
    let d_non_null = row.get(2);
    let source_j_non_null = row.get(3);
    let matches_j_formula = row.get(4);
    Ok(KdjSummary {
        has_k: has_column(client, "stock_stk_factor_pro", "kdj_k_qfq").await?,
        has_d: has_column(client, "stock_stk_factor_pro", "kdj_d_qfq").await?,
        has_source_j: has_column(client, "stock_stk_factor_pro", "kdj_qfq").await?,
        has_j_alias,
        alias_rule: "kdj_j_qfq = generated alias of kdj_qfq; kdj_qfq equals J within tolerance",
        rows,
        k_non_null,
        d_non_null,
        source_j_non_null,
        matches_j_formula,
        max_abs_delta: row.get(5),
        tolerance: KDJ_J_TOLERANCE,
        derivable_j: k_non_null == d_non_null && k_non_null > 0,
    })
}

async fn has_column(client: &Client, table_name: &str, column_name: &str) -> Result<bool> {
    let row = client
        .query_one(
            r#"
            select exists (
                select 1
                from information_schema.columns
                where table_schema = 'public'
                  and table_name = $1
                  and column_name = $2
            )
            "#,
            &[&table_name, &column_name],
        )
        .await?;
    Ok(row.get(0))
}

async fn query_calc_tables(
    client: &Client,
    pick_date: NaiveDate,
    universe_rows: i64,
) -> Result<BTreeMap<String, CalcTableSummary>> {
    let specs = [
        (
            "stock_daily_asof_indicators",
            MACD_CALC_VERSION,
            "stock_daily_asof_indicators",
        ),
        (
            "stock_daily_rolling_factors",
            ROLLING_CALC_VERSION,
            "stock_daily_rolling_factors",
        ),
        (
            "stock_daily_left_peak",
            LEFT_PEAK_CALC_VERSION,
            "stock_daily_left_peak",
        ),
    ];
    let mut summaries = BTreeMap::new();
    for (table, calc_version, key) in specs {
        let sql = format!(
            r#"select count(*)::bigint from "{table}" where "trade_date" = $1 and "calc_version" = $2"#
        );
        let rows = query_i64(client, &sql, &[&pick_date, &calc_version]).await?;
        summaries.insert(
            key.to_string(),
            CalcTableSummary {
                table,
                calc_version,
                rows,
                expected_rows: universe_rows,
                aligned_with_universe: rows == universe_rows,
            },
        );
    }
    Ok(summaries)
}

async fn query_period_state(
    client: &Client,
    pick_date: NaiveDate,
    universe_rows: i64,
    coverage_threshold: f64,
) -> Result<PeriodStateSummary> {
    let rows = client
        .query(
            r#"
            with universe as (
                select "ts_code"
                from "stock_stk_factor_pro"
                where "trade_date" = $1
            ), periods(period_type) as (
                values ('daily'), ('weekly'), ('monthly')
            ), latest_state as (
                select distinct on (u."ts_code", p.period_type)
                    u."ts_code", p.period_type, s."period_end_date", s."period_count"
                from universe u
                cross join periods p
                left join "stock_period_indicator_state" s
                  on s."ts_code" = u."ts_code"
                 and s."period_type" = p.period_type
                 and s."calc_version" = $2
                 and s."period_end_date" <= $1
                order by u."ts_code", p.period_type, s."period_end_date" desc nulls last
            )
            select
                period_type,
                count(*) filter (where period_end_date is not null)::bigint,
                count(*)::bigint
            from latest_state
            group by period_type
            order by period_type
            "#,
            &[&pick_date, &MACD_CALC_VERSION],
        )
        .await?;
    let mut by_period: BTreeMap<String, PeriodCoverage> = BTreeMap::new();
    for row in rows {
        let period_type: String = row.get(0);
        by_period.insert(period_type, PeriodCoverage::new(row.get(1), row.get(2)));
    }
    let missing_rows = client
        .query(
            r#"
            with universe as (
                select "ts_code",
                    min("trade_date") over (partition by "ts_code") as first_source_trade_date,
                    count(*) over (partition by "ts_code") as source_trade_days
                from "stock_stk_factor_pro"
                where "trade_date" <= $1
            ), latest_universe as (
                select "ts_code"
                from "stock_stk_factor_pro"
                where "trade_date" = $1
            ), periods(period_type) as (
                values ('daily'), ('weekly'), ('monthly')
            ), latest_state as (
                select distinct on (u."ts_code", p.period_type)
                    u."ts_code", p.period_type, s."period_end_date"
                from latest_universe u
                cross join periods p
                left join "stock_period_indicator_state" s
                  on s."ts_code" = u."ts_code"
                 and s."period_type" = p.period_type
                 and s."calc_version" = $2
                 and s."period_end_date" <= $1
                order by u."ts_code", p.period_type, s."period_end_date" desc nulls last
            ), missing as (
                select "ts_code",
                    array_agg(period_type order by period_type) filter (where period_end_date is null) as missing_periods
                from latest_state
                group by "ts_code"
                having count(*) filter (where period_end_date is not null) < 3
            ), source_summary as (
                select "ts_code",
                    min(first_source_trade_date) as first_source_trade_date,
                    max(source_trade_days)::bigint as source_trade_days
                from universe
                group by "ts_code"
            )
            select m."ts_code", m.missing_periods, s.first_source_trade_date, s.source_trade_days
            from missing m
            join source_summary s on s."ts_code" = m."ts_code"
            order by m."ts_code"
            "#,
            &[&pick_date, &MACD_CALC_VERSION],
        )
        .await?;
    let missing = missing_rows
        .into_iter()
        .map(|row| PeriodMissingStock {
            ts_code: row.get(0),
            missing_periods: row.get(1),
            first_source_trade_date: date_text(row.get(2)),
            source_trade_days: row.get(3),
            reason: "short source history; completed weekly/monthly state may not exist yet",
            blocking: false,
        })
        .collect::<Vec<_>>();

    Ok(PeriodStateSummary {
        calc_version: MACD_CALC_VERSION,
        coverage_threshold,
        daily: by_period
            .remove("daily")
            .unwrap_or(PeriodCoverage::empty(universe_rows)),
        weekly: by_period
            .remove("weekly")
            .unwrap_or(PeriodCoverage::empty(universe_rows)),
        monthly: by_period
            .remove("monthly")
            .unwrap_or(PeriodCoverage::empty(universe_rows)),
        acceptable_missing_count: missing.len() as i64,
        missing,
    })
}

async fn query_ths(client: &Client, pick_date: NaiveDate) -> Result<ThsSummary> {
    Ok(ThsSummary {
        index_ths_daily: QueryRows {
            rows: query_i64(
                client,
                r#"select count(*)::bigint from "index_ths_daily" where "trade_date" = $1"#,
                &[&pick_date],
            )
            .await?,
        },
        index_daily_asof_indicators: QueryRows {
            rows: query_i64(
                client,
                r#"select count(*)::bigint from "index_daily_asof_indicators" where "trade_date" = $1 and "calc_version" = $2"#,
                &[&pick_date, &MACD_CALC_VERSION],
            )
            .await?,
        },
        index_moneyflow_cnt_ths: QueryRows {
            rows: query_i64(
                client,
                r#"select count(*)::bigint from "index_moneyflow_cnt_ths" where "trade_date" = $1"#,
                &[&pick_date],
            )
            .await?,
        },
        index_ths_member: QueryRows {
            rows: query_i64(
                client,
                r#"select count(*)::bigint from "index_ths_member""#,
                &[],
            )
            .await?,
        },
    })
}

#[derive(Debug, Serialize)]
struct ContractReport {
    status: &'static str,
    contract: &'static str,
    pick_date: String,
    universe_rows: i64,
    lookback: LookbackSummary,
    adj_factor: AdjFactorSummary,
    qfq_ohlc: QfqOhlcSummary,
    kdj: KdjSummary,
    calc_tables: BTreeMap<String, CalcTableSummary>,
    period_state: PeriodStateSummary,
    ths: ThsSummary,
    blocking_failures: Vec<String>,
}

#[derive(Debug, Serialize)]
struct LookbackSummary {
    expected_trade_days: i64,
    distinct_trade_days: i64,
    start_date: Option<String>,
    end_date: Option<String>,
}

#[derive(Debug, Serialize)]
struct AdjFactorSummary {
    rows: i64,
    non_null: i64,
    positive_finite: i64,
    min: Option<f64>,
    max: Option<f64>,
}

#[derive(Debug, Serialize)]
struct QfqOhlcSummary {
    fields: BTreeMap<String, FieldCoverage>,
}

#[derive(Debug, Serialize)]
struct FieldCoverage {
    rows: i64,
    non_null: i64,
}

#[derive(Debug, Serialize)]
struct KdjSummary {
    has_k: bool,
    has_d: bool,
    has_source_j: bool,
    has_j_alias: bool,
    alias_rule: &'static str,
    rows: i64,
    k_non_null: i64,
    d_non_null: i64,
    source_j_non_null: i64,
    matches_j_formula: i64,
    max_abs_delta: Option<f64>,
    tolerance: f64,
    derivable_j: bool,
}

#[derive(Debug, Serialize)]
struct CalcTableSummary {
    table: &'static str,
    calc_version: &'static str,
    rows: i64,
    expected_rows: i64,
    aligned_with_universe: bool,
}

#[derive(Debug, Serialize)]
struct PeriodStateSummary {
    calc_version: &'static str,
    coverage_threshold: f64,
    daily: PeriodCoverage,
    weekly: PeriodCoverage,
    monthly: PeriodCoverage,
    acceptable_missing_count: i64,
    missing: Vec<PeriodMissingStock>,
}

#[derive(Debug, Serialize)]
struct PeriodCoverage {
    covered: i64,
    expected_rows: i64,
    coverage_ratio: f64,
}

impl PeriodCoverage {
    fn new(covered: i64, expected_rows: i64) -> Self {
        Self {
            covered,
            expected_rows,
            coverage_ratio: ratio(covered, expected_rows),
        }
    }

    fn empty(expected_rows: i64) -> Self {
        Self {
            covered: 0,
            expected_rows,
            coverage_ratio: 0.0,
        }
    }
}

fn ratio(numerator: i64, denominator: i64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

#[derive(Debug, Serialize)]
struct PeriodMissingStock {
    ts_code: String,
    missing_periods: Vec<String>,
    first_source_trade_date: String,
    source_trade_days: i64,
    reason: &'static str,
    blocking: bool,
}

#[derive(Debug, Serialize)]
struct ThsSummary {
    index_ths_daily: QueryRows,
    index_daily_asof_indicators: QueryRows,
    index_moneyflow_cnt_ths: QueryRows,
    index_ths_member: QueryRows,
}

#[derive(Debug, Serialize)]
struct QueryRows {
    rows: i64,
}

fn date_text(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

fn optional_date_text(date: Option<NaiveDate>) -> Option<String> {
    date.map(date_text)
}
