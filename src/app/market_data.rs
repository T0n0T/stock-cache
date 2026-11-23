//! Market-data date windows, fetch planning, and cache coverage.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufWriter, Write},
    path::Path,
};

use anyhow::{ensure, Context, Result};
use chrono::{Datelike, Duration, FixedOffset, NaiveDate, Utc};
use duckdb_neo::{connection::Connection, Parameters};
use serde_json::{json, Value};
use stock_cache::{
    metadata::tushare_fields::{dataset, DATASETS},
    storage::duckdb,
    TushareHttpClient,
};

pub(super) const CALC_SOURCES: [&str; 2] = ["stock_stk_factor_pro", "index_idx_factor_pro"];
const DEFAULT_TABLES: &str = include_str!("../../config/target-cache-tables.txt");
const DEFAULT_INDEXES: &str = include_str!("../../config/default-indexes.csv");

pub(super) fn date(value: &str) -> Result<NaiveDate> {
    ensure!(value.len() == 10, "date must use YYYY-MM-DD: {value}");
    Ok(NaiveDate::parse_from_str(value, "%Y-%m-%d")?)
}
pub(super) fn literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
pub(super) fn strings(connection: &Connection, sql: impl AsRef<str>) -> Result<Vec<String>> {
    let mut result = connection.query(sql.as_ref(), Parameters::None)?;
    let mut values = Vec::new();
    while let Some(chunk) = result.next_chunk()? {
        let column = chunk.get_vector_at::<String>(0)?.reader()?;
        for index in 0..chunk.row_count()? {
            if let Some(value) = column.get(index)? {
                values.push(value.to_string());
            }
        }
    }
    Ok(values)
}

pub(super) fn row_counts(
    connection: &Connection,
    table: &str,
    days: &[NaiveDate],
    code: &str,
) -> Result<BTreeMap<NaiveDate, i64>> {
    let dates = days
        .iter()
        .map(|day| format!("DATE '{day}'"))
        .collect::<Vec<_>>()
        .join(",");
    let code_filter = if code.is_empty() {
        String::new()
    } else {
        format!(" AND ts_code = {}", literal(code))
    };
    let sql = format!(
        "SELECT trade_date::VARCHAR, count(*)::BIGINT FROM {table} \
         WHERE trade_date IN ({dates}){code_filter} GROUP BY trade_date"
    );
    let mut result = connection.query(&sql, Parameters::None)?;
    let mut counts = BTreeMap::new();
    while let Some(chunk) = result.next_chunk()? {
        let dates = chunk.get_vector_at::<String>(0)?.reader()?;
        let values = chunk.get_vector_at::<i64>(1)?.reader()?;
        for index in 0..chunk.row_count()? {
            if let Some(day) = dates.get(index)? {
                counts.insert(date(day)?, values.get(index)?.copied().unwrap_or(0));
            }
        }
    }
    Ok(counts)
}
pub(super) fn execute(connection: &Connection, sql: impl AsRef<str>) -> Result<()> {
    connection.execute(sql.as_ref(), Parameters::None)?;
    Ok(())
}

pub(super) fn targets(
    open: &[NaiveDate],
    start: Option<NaiveDate>,
    end: NaiveDate,
    days: u32,
) -> Result<Vec<NaiveDate>> {
    let mut selected = open
        .iter()
        .copied()
        .filter(|d| *d <= end && start.is_none_or(|s| *d >= s))
        .collect::<Vec<_>>();
    if start.is_none() {
        ensure!(
            selected.len() >= days as usize,
            "calendar has too few trading days for requested lookback"
        );
        selected = selected.split_off(selected.len() - days as usize);
    }
    ensure!(
        !selected.is_empty(),
        "requested window contains no trading days"
    );
    Ok(selected)
}

pub(super) fn env_number(key: &str, default: usize) -> Result<usize> {
    std::env::var(key)
        .ok()
        .map(|v| v.parse().with_context(|| format!("invalid {key}")))
        .unwrap_or(Ok(default))
}
pub(super) fn configured_tables() -> Result<Vec<&'static str>> {
    let text = std::env::var("TARGET_CACHE_TABLES_PATH")
        .ok()
        .map(fs::read_to_string)
        .transpose()?
        .unwrap_or_else(|| DEFAULT_TABLES.into());
    let mut tables = Vec::new();
    for id in text
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
    {
        let metadata = dataset(id).with_context(|| format!("unknown target cache table: {id}"))?;
        ensure!(
            metadata.fetch_plan.kind.starts_with("by_trade_date"),
            "{id} is not a date-based cache table"
        );
        tables.push(metadata.id);
    }
    ensure!(
        CALC_SOURCES.iter().all(|id| tables.contains(id)),
        "target tables must include all calculation sources"
    );
    Ok(tables)
}
fn configured_index_codes(category: Option<&str>) -> Result<Vec<String>> {
    let text = std::env::var("INDEX_LIST_PATH")
        .ok()
        .map(fs::read_to_string)
        .transpose()?
        .unwrap_or_else(|| DEFAULT_INDEXES.into());
    let mut codes = BTreeSet::new();
    for line in text.lines().skip(1) {
        let columns = line.split(',').map(str::trim).collect::<Vec<_>>();
        ensure!(columns.len() == 4, "invalid index-list CSV row");
        if columns[3].eq_ignore_ascii_case("true")
            && category.is_none_or(|wanted| columns[2].eq_ignore_ascii_case(wanted))
        {
            codes.insert(columns[0].to_string());
        }
    }
    ensure!(
        !codes.is_empty() || category.is_some(),
        "index list has no enabled codes"
    );
    Ok(codes.into_iter().collect())
}
pub(super) fn configured_indexes() -> Result<Vec<String>> {
    configured_index_codes(None)
}
pub(super) fn configured_market_indexes() -> Result<Vec<String>> {
    configured_index_codes(Some("market"))
}
#[derive(Clone)]
pub(super) struct Job {
    pub(super) id: &'static str,
    /// First requested day; also the coverage key day for single-day jobs.
    pub(super) day: NaiveDate,
    /// Every day this request must write and mark covered, sorted ascending.
    pub(super) days: Vec<NaiveDate>,
    pub(super) code: String,
}
impl Job {
    pub(super) fn single(id: &'static str, day: NaiveDate, code: String) -> Self {
        Self {
            id,
            day,
            days: vec![day],
            code,
        }
    }
    pub(super) fn key(&self) -> String {
        format!("{}|{}|{}", self.id, self.day, self.code)
    }
    pub(super) fn params(&self) -> BTreeMap<String, Value> {
        if self.code.is_empty() {
            BTreeMap::from([(
                "trade_date".into(),
                json!(self.day.format("%Y%m%d").to_string()),
            )])
        } else {
            let start = self.days.first().copied().unwrap_or(self.day);
            let end = self.days.last().copied().unwrap_or(self.day);
            BTreeMap::from([
                ("ts_code".into(), json!(self.code)),
                (
                    "start_date".into(),
                    json!(start.format("%Y%m%d").to_string()),
                ),
                ("end_date".into(), json!(end.format("%Y%m%d").to_string())),
            ])
        }
    }
}

/// Group sorted trading days into provider-friendly request ranges. Adjacent
/// sessions stay together (holiday gaps included); long runs are capped so a
/// single response remains small enough to page reliably.
pub(super) fn group_days(mut days: Vec<NaiveDate>) -> Vec<Vec<NaiveDate>> {
    const MAX_GAP_DAYS: i64 = 10;
    const MAX_RANGE_DAYS: usize = 250;
    days.sort_unstable();
    days.dedup();
    let mut groups: Vec<Vec<NaiveDate>> = Vec::new();
    let mut current: Vec<NaiveDate> = Vec::new();
    for day in days {
        let split = current.last().is_some_and(|previous| {
            (day - *previous).num_days() > MAX_GAP_DAYS || current.len() >= MAX_RANGE_DAYS
        });
        if split {
            groups.push(std::mem::take(&mut current));
        }
        current.push(day);
    }
    if !current.is_empty() {
        groups.push(current);
    }
    groups
}
pub(super) fn write_rows(path: &Path, rows: &[BTreeMap<String, Value>]) -> Result<()> {
    let mut file = BufWriter::new(fs::File::create(path)?);
    for row in rows {
        serde_json::to_writer(&mut file, row)?;
        file.write_all(b"\n")?;
    }
    file.flush()?;
    Ok(())
}
pub(super) fn refresh(
    runtime: &tokio::runtime::Runtime,
    client: &TushareHttpClient,
    database: &Path,
    root: &Path,
    counts: &mut BTreeMap<String, usize>,
) -> Result<()> {
    for metadata in DATASETS
        .iter()
        .filter(|d| d.fetch_plan.kind.starts_with("refresh_current"))
    {
        let param_sets = if metadata.id == "index_ths_member" {
            duckdb::with_connection(&database.to_string_lossy(), |c| {
                strings(c, "SELECT ts_code FROM index_ths ORDER BY ts_code")
            })?
            .into_iter()
            .map(|code| BTreeMap::from([("ts_code".into(), json!(code))]))
            .collect::<Vec<_>>()
        } else if metadata.id == "index_sw_member" {
            // Current membership and expired members are separate provider
            // snapshots.  Keep both so in_date/out_date remain point-in-time.
            ["Y", "N"]
                .into_iter()
                .map(|flag| BTreeMap::from([("is_new".into(), json!(flag))]))
                .collect()
        } else if metadata.id == "index_sw_classify" {
            vec![BTreeMap::from([("src".into(), json!("SW2021"))])]
        } else {
            vec![BTreeMap::new()]
        };
        let mut all = Vec::new();
        for params in param_sets {
            all.extend(runtime.block_on(client.fetch_dataset(metadata, &params))?);
        }
        ensure!(
            !all.is_empty(),
            "refusing to replace {} with empty snapshot",
            metadata.id
        );
        let path = root.join("refresh.jsonl");
        write_rows(&path, &all)?;
        duckdb::with_connection(&database.to_string_lossy(), |connection| {
            // Replace each reference snapshot in one writer transaction.
            duckdb::write_jsonl(
                connection,
                metadata.table,
                &path,
                "replace-all",
                &[],
                &BTreeMap::new(),
                &[],
            )?;
            Ok(())
        })?;
        counts.insert(metadata.id.into(), all.len());
    }
    for metadata in DATASETS
        .iter()
        .filter(|d| d.fetch_plan.kind == "refresh_weight_history")
    {
        let codes = configured_market_indexes()?;
        if codes.is_empty() {
            continue;
        }
        let today = Utc::now()
            .with_timezone(&FixedOffset::east_opt(8 * 3600).expect("valid offset"))
            .date_naive();
        let mut all = Vec::new();
        for code in codes {
            let mut start = NaiveDate::from_ymd_opt(2020, 1, 1).expect("valid date");
            while start <= today {
                let next_year =
                    NaiveDate::from_ymd_opt(start.year() + 1, 1, 1).expect("valid date");
                let end = (next_year - Duration::days(1)).min(today);
                let params = BTreeMap::from([
                    ("index_code".into(), json!(code)),
                    (
                        "start_date".into(),
                        json!(start.format("%Y%m%d").to_string()),
                    ),
                    ("end_date".into(), json!(end.format("%Y%m%d").to_string())),
                ]);
                let rows = runtime.block_on(client.fetch_dataset(metadata, &params))?;
                for row in &rows {
                    ensure!(
                        row.get("index_code").and_then(Value::as_str) == Some(&code),
                        "index_weight response code mismatch"
                    );
                }
                all.extend(rows);
                start = end + Duration::days(1);
            }
        }
        ensure!(
            !all.is_empty(),
            "refusing to replace {} with empty snapshot",
            metadata.id
        );
        let path = root.join("refresh.jsonl");
        write_rows(&path, &all)?;
        duckdb::with_connection(&database.to_string_lossy(), |connection| {
            duckdb::write_jsonl(
                connection,
                metadata.table,
                &path,
                "replace-batch-keys",
                &[],
                &BTreeMap::new(),
                &[],
            )?;
            Ok(())
        })?;
        counts.insert(metadata.id.into(), all.len());
    }
    Ok(())
}

// A coverage marker is reusable only while its source rows still exist. The
// per-code key prevents a newly enabled index inheriting another index's coverage.
pub(super) fn coverage(connection: &Connection) -> Result<BTreeSet<String>> {
    let mut present = BTreeSet::new();
    for metadata in DATASETS
        .iter()
        .filter(|d| d.fetch_plan.kind.starts_with("by_trade_date"))
    {
        let table = metadata.table;
        present.extend(strings(connection, format!("SELECT dataset || '|' || trade_date::VARCHAR || '|' || code FROM cache_fetch_coverage c WHERE dataset={} AND row_count=(SELECT count(*) FROM {table} s WHERE s.trade_date=c.trade_date AND (c.code='' OR s.ts_code=c.code))", literal(metadata.id)))?);
    }
    Ok(present)
}
