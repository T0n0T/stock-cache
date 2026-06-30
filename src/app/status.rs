use std::collections::BTreeMap;

use anyhow::Result;
use chrono::{Months, NaiveDate};
use serde::Serialize;
use tokio_postgres::{Client, NoTls, Row};

use crate::app::{config::Config, database_state};

const MACD_CALC_VERSION: &str = "macd_qfq_12_26_9_v1";
const ROLLING_CALC_VERSION: &str = "rolling_qfq_v1";
const LEFT_PEAK_CALC_VERSION: &str = "left_peak_qfq_v1";
const MONTHLY_MACD_WARMUP_MONTHS: u32 = 36;

const SOURCE_TABLES: &[TableDateSpec] = &[
    TableDateSpec::trade_date("stock_stk_factor_pro"),
    TableDateSpec::trade_date("stock_moneyflow_ths"),
    TableDateSpec::trade_date("index_idx_factor_pro"),
    TableDateSpec::trade_date("index_ths_daily"),
    TableDateSpec::trade_date("index_moneyflow_cnt_ths"),
    TableDateSpec::trade_date("index_limit_cpt_list"),
    TableDateSpec::trade_date("stock_limit_list_ths"),
];

const CLASSIFICATION_TABLES: &[&str] = &["index_ths", "index_ths_member"];

const CALC_TABLES: &[CalcTableSpec] = &[
    CalcTableSpec::with_period_breakdown(
        "stock_period_indicator_state",
        "period_end_date",
        Some(MACD_CALC_VERSION),
    ),
    CalcTableSpec::with_period_breakdown(
        "index_period_indicator_state",
        "period_end_date",
        Some(MACD_CALC_VERSION),
    ),
    CalcTableSpec::new(
        "stock_daily_asof_indicators",
        "trade_date",
        Some(MACD_CALC_VERSION),
    ),
    CalcTableSpec::new(
        "index_daily_asof_indicators",
        "trade_date",
        Some(MACD_CALC_VERSION),
    ),
    CalcTableSpec::new(
        "stock_daily_rolling_factors",
        "trade_date",
        Some(ROLLING_CALC_VERSION),
    ),
    CalcTableSpec::new(
        "stock_daily_left_peak",
        "trade_date",
        Some(LEFT_PEAK_CALC_VERSION),
    ),
];

#[derive(Debug, Clone, Copy)]
struct TableDateSpec {
    table: &'static str,
    date_field: &'static str,
}

impl TableDateSpec {
    const fn trade_date(table: &'static str) -> Self {
        Self {
            table,
            date_field: "trade_date",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct CalcTableSpec {
    table: &'static str,
    date_field: &'static str,
    calc_version: Option<&'static str>,
    period_breakdown: bool,
}

impl CalcTableSpec {
    const fn new(
        table: &'static str,
        date_field: &'static str,
        calc_version: Option<&'static str>,
    ) -> Self {
        Self {
            table,
            date_field,
            calc_version,
            period_breakdown: false,
        }
    }

    const fn with_period_breakdown(
        table: &'static str,
        date_field: &'static str,
        calc_version: Option<&'static str>,
    ) -> Self {
        Self {
            table,
            date_field,
            calc_version,
            period_breakdown: true,
        }
    }
}

#[derive(Debug, Serialize)]
struct StatusSummary {
    status: &'static str,
    source_tables: BTreeMap<String, DateTableStatus>,
    classification_tables: BTreeMap<String, CountTableStatus>,
    calc_tables: BTreeMap<String, DateTableStatus>,
    warmup: BTreeMap<String, WarmupStatus>,
}

#[derive(Debug, Clone, Serialize)]
struct DateTableStatus {
    row_count: i64,
    min_date: Option<String>,
    max_date: Option<String>,
    distinct_dates: i64,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    periods: BTreeMap<String, PeriodTableStatus>,
}

#[derive(Debug, Clone, Serialize)]
struct PeriodTableStatus {
    row_count: i64,
    min_date: Option<String>,
    max_date: Option<String>,
    distinct_dates: i64,
}

#[derive(Debug, Clone, Serialize)]
struct CountTableStatus {
    row_count: i64,
}

#[derive(Debug, Clone, Serialize)]
struct WarmupStatus {
    source_table: &'static str,
    calc_table: &'static str,
    target_start_date: Option<String>,
    target_end_date: Option<String>,
    required_source_start_date: Option<String>,
    warmup_start_date: Option<String>,
    warmup_end_date: Option<String>,
    warmup_distinct_dates: i64,
    warmup_row_count: i64,
    has_required_start: bool,
}

pub async fn run(config: &Config) -> Result<()> {
    let (client, connection) = tokio_postgres::connect(&config.postgres_dsn, NoTls).await?;
    let connection_task = tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("postgres connection error: {error}");
        }
    });

    let result = status_summary(&client).await;
    connection_task.abort();
    let summary = result?;
    let summary_json = serde_json::to_string_pretty(&summary)?;
    database_state::write_json(&summary_json)?;
    println!("{summary_json}");
    Ok(())
}

async fn status_summary(client: &Client) -> Result<StatusSummary> {
    let mut source_tables = BTreeMap::new();
    for spec in SOURCE_TABLES {
        source_tables.insert(
            spec.table.to_string(),
            load_date_table_status(client, *spec).await?,
        );
    }

    let mut classification_tables = BTreeMap::new();
    for table in CLASSIFICATION_TABLES {
        classification_tables.insert(
            (*table).to_string(),
            load_count_table_status(client, table).await?,
        );
    }

    let mut calc_tables = BTreeMap::new();
    for spec in CALC_TABLES {
        calc_tables.insert(
            spec.table.to_string(),
            load_calc_table_status(client, *spec).await?,
        );
    }

    let mut warmup = BTreeMap::new();
    warmup.insert(
        "stock_macd".to_string(),
        load_macd_warmup_status(
            client,
            WarmupSourceSpec::table("stock_stk_factor_pro"),
            "stock_daily_asof_indicators",
        )
        .await?,
    );
    warmup.insert(
        "index_macd".to_string(),
        load_macd_warmup_status(
            client,
            WarmupSourceSpec::combined_index(),
            "index_daily_asof_indicators",
        )
        .await?,
    );

    Ok(StatusSummary {
        status: "success",
        source_tables,
        classification_tables,
        calc_tables,
        warmup,
    })
}

async fn load_date_table_status(client: &Client, spec: TableDateSpec) -> Result<DateTableStatus> {
    let sql = format!(
        "select count(*)::bigint, min({date}), max({date}), count(distinct {date})::bigint from {table}",
        date = quote_ident(spec.date_field),
        table = quote_ident(spec.table),
    );
    let row = client.query_one(&sql, &[]).await?;
    Ok(date_table_status_from_row(&row))
}

async fn load_count_table_status(client: &Client, table: &str) -> Result<CountTableStatus> {
    let sql = format!("select count(*)::bigint from {}", quote_ident(table));
    let row = client.query_one(&sql, &[]).await?;
    Ok(CountTableStatus {
        row_count: row.get::<_, i64>(0),
    })
}

async fn load_calc_table_status(client: &Client, spec: CalcTableSpec) -> Result<DateTableStatus> {
    let sql = if spec.calc_version.is_some() {
        format!(
            "select count(*)::bigint, min({date}), max({date}), count(distinct {date})::bigint from {table} where \"calc_version\" = $1",
            date = quote_ident(spec.date_field),
            table = quote_ident(spec.table),
        )
    } else {
        format!(
            "select count(*)::bigint, min({date}), max({date}), count(distinct {date})::bigint from {table}",
            date = quote_ident(spec.date_field),
            table = quote_ident(spec.table),
        )
    };
    let row = if let Some(calc_version) = spec.calc_version {
        client.query_one(&sql, &[&calc_version]).await?
    } else {
        client.query_one(&sql, &[]).await?
    };
    let mut status = date_table_status_from_row(&row);
    if spec.period_breakdown {
        status.periods = load_calc_period_table_statuses(client, spec).await?;
    }
    Ok(status)
}

async fn load_calc_period_table_statuses(
    client: &Client,
    spec: CalcTableSpec,
) -> Result<BTreeMap<String, PeriodTableStatus>> {
    let sql = if spec.calc_version.is_some() {
        format!(
            "select \"period_type\", count(*)::bigint, min({date}), max({date}), count(distinct {date})::bigint from {table} where \"calc_version\" = $1 group by \"period_type\" order by \"period_type\"",
            date = quote_ident(spec.date_field),
            table = quote_ident(spec.table),
        )
    } else {
        format!(
            "select \"period_type\", count(*)::bigint, min({date}), max({date}), count(distinct {date})::bigint from {table} group by \"period_type\" order by \"period_type\"",
            date = quote_ident(spec.date_field),
            table = quote_ident(spec.table),
        )
    };
    let rows = if let Some(calc_version) = spec.calc_version {
        client.query(&sql, &[&calc_version]).await?
    } else {
        client.query(&sql, &[]).await?
    };
    let mut statuses = BTreeMap::new();
    for row in rows {
        let period_type = row.get::<_, String>(0);
        let min_date = row.get::<_, Option<NaiveDate>>(2);
        let max_date = row.get::<_, Option<NaiveDate>>(3);
        statuses.insert(
            period_type,
            PeriodTableStatus {
                row_count: row.get(1),
                min_date: min_date.map(format_date),
                max_date: max_date.map(format_date),
                distinct_dates: row.get(4),
            },
        );
    }
    Ok(statuses)
}

fn date_table_status_from_row(row: &Row) -> DateTableStatus {
    let min_date = row.get::<_, Option<NaiveDate>>(1);
    let max_date = row.get::<_, Option<NaiveDate>>(2);
    DateTableStatus {
        row_count: row.get(0),
        min_date: min_date.map(format_date),
        max_date: max_date.map(format_date),
        distinct_dates: row.get(3),
        periods: BTreeMap::new(),
    }
}

async fn load_macd_warmup_status(
    client: &Client,
    source: WarmupSourceSpec,
    calc_table: &'static str,
) -> Result<WarmupStatus> {
    let target = load_calc_table_status(
        client,
        CalcTableSpec::new(calc_table, "trade_date", Some(MACD_CALC_VERSION)),
    )
    .await?;
    let target_start = parse_status_date(target.min_date.as_deref());
    let target_end = parse_status_date(target.max_date.as_deref());
    let required_start = target_start
        .and_then(|date| date.checked_sub_months(Months::new(MONTHLY_MACD_WARMUP_MONTHS)));
    let warmup = match target_start {
        Some(start) => load_source_before_target(client, source, start).await?,
        None => WarmupCoverage::empty(),
    };
    Ok(warmup_status(
        source.label,
        calc_table,
        target_start,
        target_end,
        required_start,
        warmup,
    ))
}

#[derive(Debug, Clone, Copy)]
struct WarmupSourceSpec {
    label: &'static str,
    kind: WarmupSourceKind,
}

impl WarmupSourceSpec {
    const fn table(table: &'static str) -> Self {
        Self {
            label: table,
            kind: WarmupSourceKind::Table(table),
        }
    }

    const fn combined_index() -> Self {
        Self {
            label: "index_ths_daily + index_idx_factor_pro",
            kind: WarmupSourceKind::CombinedIndex,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum WarmupSourceKind {
    Table(&'static str),
    CombinedIndex,
}

async fn load_source_before_target(
    client: &Client,
    source: WarmupSourceSpec,
    target_start: NaiveDate,
) -> Result<WarmupCoverage> {
    let sql = match source.kind {
        WarmupSourceKind::Table(table) => format!(
            "select count(*)::bigint, min(\"trade_date\"), max(\"trade_date\"), count(distinct \"trade_date\")::bigint from {} where \"trade_date\" < $1",
            quote_ident(table),
        ),
        WarmupSourceKind::CombinedIndex => {
            r#"
            select count(*)::bigint, min("trade_date"), max("trade_date"), count(distinct "trade_date")::bigint
            from (
                select "trade_date" from "index_ths_daily" where "trade_date" < $1
                union all
                select "trade_date" from "index_idx_factor_pro" where "trade_date" < $1
            ) index_source
            "#.to_string()
        }
    };
    let row = client.query_one(&sql, &[&target_start]).await?;
    Ok(WarmupCoverage {
        row_count: row.get(0),
        start: row.get(1),
        end: row.get(2),
        distinct_dates: row.get(3),
    })
}

#[derive(Debug, Clone, Copy)]
struct WarmupCoverage {
    row_count: i64,
    start: Option<NaiveDate>,
    end: Option<NaiveDate>,
    distinct_dates: i64,
}

impl WarmupCoverage {
    fn empty() -> Self {
        Self {
            row_count: 0,
            start: None,
            end: None,
            distinct_dates: 0,
        }
    }
}

fn warmup_status(
    source_table: &'static str,
    calc_table: &'static str,
    target_start: Option<NaiveDate>,
    target_end: Option<NaiveDate>,
    required_start: Option<NaiveDate>,
    warmup: WarmupCoverage,
) -> WarmupStatus {
    WarmupStatus {
        source_table,
        calc_table,
        target_start_date: target_start.map(format_date),
        target_end_date: target_end.map(format_date),
        required_source_start_date: required_start.map(format_date),
        warmup_start_date: warmup.start.map(format_date),
        warmup_end_date: warmup.end.map(format_date),
        warmup_distinct_dates: warmup.distinct_dates,
        warmup_row_count: warmup.row_count,
        has_required_start: match (warmup.start, required_start) {
            (Some(start), Some(required)) => start <= required,
            _ => false,
        },
    }
}

fn parse_status_date(value: Option<&str>) -> Option<NaiveDate> {
    value.and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
}

fn format_date(date: NaiveDate) -> String {
    date.to_string()
}

fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warmup_status_reports_required_start_coverage() {
        let target_start = NaiveDate::from_ymd_opt(2026, 6, 1);
        let required_start = NaiveDate::from_ymd_opt(2023, 6, 1);
        let status = warmup_status(
            "stock_stk_factor_pro",
            "stock_daily_asof_indicators",
            target_start,
            NaiveDate::from_ymd_opt(2026, 6, 26),
            required_start,
            WarmupCoverage {
                row_count: 100,
                start: NaiveDate::from_ymd_opt(2023, 6, 1),
                end: NaiveDate::from_ymd_opt(2026, 5, 29),
                distinct_dates: 724,
            },
        );

        assert!(status.has_required_start);
        assert_eq!(
            status.required_source_start_date.as_deref(),
            Some("2023-06-01")
        );
        assert_eq!(status.warmup_start_date.as_deref(), Some("2023-06-01"));
        assert_eq!(status.warmup_end_date.as_deref(), Some("2026-05-29"));
    }

    #[test]
    fn warmup_status_flags_short_history() {
        let status = warmup_status(
            "index_ths_daily",
            "index_daily_asof_indicators",
            NaiveDate::from_ymd_opt(2026, 6, 1),
            NaiveDate::from_ymd_opt(2026, 6, 26),
            NaiveDate::from_ymd_opt(2023, 6, 1),
            WarmupCoverage {
                row_count: 100,
                start: NaiveDate::from_ymd_opt(2024, 1, 1),
                end: NaiveDate::from_ymd_opt(2026, 5, 29),
                distinct_dates: 580,
            },
        );

        assert!(!status.has_required_start);
    }
}
