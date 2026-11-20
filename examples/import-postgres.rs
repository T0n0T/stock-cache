//! One-off migration of Tushare source tables from the old PostgreSQL cache.
//!
//! This is a development helper, not a product CLI. It imports only the source
//! and reference datasets (never `calc_*` results), then rebuilds
//! `cache_fetch_coverage` so `write --mode full` does not refetch migrated
//! history. Calculations are always rebuilt by the Rust implementation.
//!
//! Usage:
//!
//! ```bash
//! cargo run --release --example import-postgres -- \
//!   --dsn 'postgresql://user:password@host:5432/db' \
//!   --database runtime/stock-cache.duckdb
//! ```
//!
//! The DuckDB postgres extension is downloaded on first use, so this example
//! needs network access to `extensions.duckdb.org`.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::{bail, ensure, Context, Result};
use chrono::{Datelike, Duration, NaiveDate, Utc};
use clap::Parser;
use duckdb_neo::{connection::Connection, Parameters};
use stock_cache::{
    metadata::tushare_fields::{DatasetMetadata, WriteStrategy, DATASETS},
    storage::duckdb,
};

#[derive(Debug, Parser)]
#[command(
    name = "import-postgres",
    about = "Import Tushare source tables from PostgreSQL"
)]
struct Args {
    /// PostgreSQL DSN, e.g. postgresql://user:password@host:5432/database.
    #[arg(long)]
    dsn: Option<String>,
    /// Target DuckDB database (default: DUCKDB_PATH or runtime/stock-cache.duckdb).
    #[arg(long)]
    database: Option<PathBuf>,
    /// Import only these dataset ids (comma-separated; default: all present).
    #[arg(long, value_delimiter = ',')]
    tables: Vec<String>,
    /// Show the plan and row counts without writing.
    #[arg(long)]
    dry_run: bool,
    /// Do not rebuild cache_fetch_coverage.
    #[arg(long)]
    skip_coverage: bool,
    /// Rebuild the trading calendar and coverage from existing DuckDB tables.
    #[arg(long)]
    calendar_only: bool,
    /// Inclusive lower bound for date-bearing tables (YYYY-MM-DD).
    #[arg(long)]
    since: Option<String>,
    /// Inclusive upper bound (YYYY-MM-DD); defaults to the latest cached date.
    #[arg(long)]
    until: Option<String>,
    /// Keep rows whose date is absent from the cached trading calendar.
    #[arg(long)]
    keep_off_calendar: bool,
    /// Never delete or overwrite existing DuckDB rows; skip duplicates.
    #[arg(long)]
    insert_only: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let dsn = args
        .dsn
        .clone()
        .or_else(|| std::env::var("STOCK_CACHE_PG_DSN").ok())
        .filter(|value| !value.is_empty());
    if !args.calendar_only && dsn.is_none() {
        bail!("--dsn or STOCK_CACHE_PG_DSN is required");
    }
    let database = args
        .database
        .clone()
        .or_else(|| std::env::var_os("DUCKDB_PATH").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("runtime/stock-cache.duckdb"));
    let selected = args.tables.iter().cloned().collect::<BTreeSet<_>>();
    for id in &selected {
        ensure!(
            DATASETS.iter().any(|metadata| metadata.id == id),
            "unknown dataset id: {id}"
        );
    }
    run(&database, dsn.as_deref(), &args, &selected)
}

fn run(database: &Path, dsn: Option<&str>, args: &Args, selected: &BTreeSet<String>) -> Result<()> {
    if args.calendar_only {
        return rebuild_existing(database, args, selected);
    }
    let dsn = dsn.context("--dsn or STOCK_CACHE_PG_DSN is required")?;
    let connection_string = normalize_dsn(dsn)?;
    println!("Importing into {}", database.display());
    println!("Source PostgreSQL: {}", redact_dsn(dsn));

    let imported = duckdb::with_connection(&database.to_string_lossy(), |connection| {
        duckdb::create_schema(connection)?;
        load_postgres(connection)?;
        connection
            .execute(
                format!(
                    "ATTACH {} AS pg (TYPE postgres, READ_ONLY)",
                    sql_string(&connection_string)
                ),
                Parameters::None,
            )
            .with_context(|| format!("attach PostgreSQL database {}", redact_dsn(dsn)))?;
        // Bulk loading does not need the input order; this lowers memory use.
        connection.execute("SET preserve_insertion_order = false", Parameters::None)?;

        let since = args.since.as_deref().map(parse_date).transpose()?;
        let until = match args.until.as_deref().map(parse_date).transpose()? {
            Some(date) => Some(date),
            None => latest_cached_date(connection)?,
        };
        if let (Some(since), Some(until)) = (since, until) {
            ensure!(since <= until, "--since must be <= --until");
        }
        let filter = DateFilter {
            since,
            until,
            calendar: calendar_bounds(connection)?,
            skip_off_calendar: !args.keep_off_calendar,
        };
        match (since, until) {
            (Some(since), Some(until)) => println!("Date range: {since}..{until}"),
            (Some(since), None) => println!("Date range: {since}.."),
            (None, Some(until)) => println!("Date range: ..{until} (latest cached date)"),
            (None, None) => println!("Date range: all"),
        }
        if args.insert_only {
            println!("Mode: insert-only (existing rows are kept)");
        } else if let (Some(since), Some(until)) = (since, until) {
            println!("Existing DuckDB rows in {since}..{until} will be replaced.");
        }
        if filter.skip_off_calendar && filter.calendar.is_none() {
            println!(
                "Warning: cache_trade_calendar is empty; non-trading rows cannot be filtered."
            );
        }

        let result = (|| -> Result<Vec<(&'static DatasetMetadata, i64)>> {
            let mut imported = Vec::new();
            for metadata in DATASETS {
                if !selected.is_empty() && !selected.contains(metadata.id) {
                    continue;
                }
                let pg_table = format!("pg.public.{}", ident(metadata.table));
                let pg_columns = match describe(connection, &pg_table) {
                    Ok(columns) => columns,
                    Err(error) => {
                        println!(
                            "  skip {}: not readable from PostgreSQL ({error})",
                            metadata.id
                        );
                        continue;
                    }
                };
                let local_columns = describe(connection, &ident(metadata.table))?;
                let columns = matching_columns(metadata, &local_columns, &pg_columns)?;
                let where_clause = if metadata.field("trade_date").is_some() {
                    filter.where_clause()
                } else {
                    String::new()
                };
                eprintln!("  {}: counting PostgreSQL rows...", metadata.id);
                let rows = table_rows(connection, &pg_table, &where_clause)?;

                if args.dry_run {
                    println!(
                        "  {}: {} PostgreSQL rows, {} matching columns",
                        metadata.id,
                        rows,
                        columns.len()
                    );
                    continue;
                }

                let empty = duckdb::table_count(connection, metadata.table)? == 0;
                eprintln!("  {}: importing {} rows", metadata.id, rows);
                let started = Instant::now();
                import_dataset(
                    connection,
                    metadata,
                    &pg_table,
                    ImportPlan {
                        columns: &columns,
                        incoming: rows,
                        empty,
                        filter: &filter,
                        insert_only: args.insert_only,
                    },
                )?;
                let written = duckdb::table_count(connection, metadata.table)?;
                eprintln!(
                    "  {}: {} DuckDB rows total ({:.1}s)",
                    metadata.id,
                    written,
                    started.elapsed().as_secs_f64()
                );
                imported.push((metadata, written));
            }
            ensure!(
                !imported.is_empty() || args.dry_run,
                "no source datasets were imported; check the DSN and table names"
            );
            if !args.dry_run {
                let pruned = if args.insert_only {
                    0
                } else {
                    prune_off_calendar(connection, &imported, &filter)?
                };
                if pruned > 0 {
                    println!("Off-calendar rows removed: {pruned}");
                }
                let calendar = rebuild_calendar(connection, &imported)?;
                println!("Calendar rows written: {calendar}");
                if !args.skip_coverage {
                    let coverage = rebuild_coverage(connection, &imported, Some(&filter))?;
                    println!("Coverage rows written: {coverage}");
                }
            }
            Ok(imported)
        })();

        let detach = connection.execute("DETACH pg", Parameters::None);
        let imported = result?;
        detach?;
        Ok(imported)
    })?;

    if args.dry_run {
        println!("Dry run complete; nothing was written.");
    } else {
        let total: i64 = imported.iter().map(|(_, rows)| rows).sum();
        println!(
            "Imported {} datasets ({} DuckDB rows).",
            imported.len(),
            total
        );
    }
    Ok(())
}

fn rebuild_existing(database: &Path, args: &Args, selected: &BTreeSet<String>) -> Result<()> {
    println!("Rebuilding calendar/coverage in {}", database.display());
    duckdb::with_connection(&database.to_string_lossy(), |connection| {
        duckdb::create_schema(connection)?;
        let present = present_datasets(connection, selected)?;
        ensure!(
            !present.is_empty(),
            "no imported datasets found in {}",
            database.display()
        );
        let calendar = rebuild_calendar(connection, &present)?;
        println!("Calendar rows written: {calendar}");
        if !args.skip_coverage {
            let coverage = rebuild_coverage(connection, &present, None)?;
            println!("Coverage rows written: {coverage}");
        }
        Ok(())
    })
}

fn present_datasets(
    connection: &Connection,
    selected: &BTreeSet<String>,
) -> Result<Vec<(&'static DatasetMetadata, i64)>> {
    let mut present = Vec::new();
    for metadata in DATASETS {
        if !selected.is_empty() && !selected.contains(metadata.id) {
            continue;
        }
        let rows = duckdb::table_count(connection, metadata.table)?;
        if rows > 0 {
            present.push((metadata, rows));
        }
    }
    Ok(present)
}

/// Remove rows already present in DuckDB whose date is not a real session.
/// Import filtering prevents new ones, but a previous import may have left them.
fn prune_off_calendar(
    connection: &Connection,
    datasets: &[(&DatasetMetadata, i64)],
    filter: &DateFilter,
) -> Result<usize> {
    if !filter.skip_off_calendar {
        return Ok(0);
    }
    let Some((calendar_first, calendar_last)) = filter.calendar else {
        return Ok(0);
    };
    let first = filter
        .since
        .map_or(calendar_first, |since| since.max(calendar_first));
    let last = filter
        .until
        .map_or(calendar_last, |until| until.min(calendar_last));
    if first > last {
        return Ok(0);
    }
    let mut total = 0usize;
    for (metadata, _) in datasets {
        if metadata.field("trade_date").is_none() {
            continue;
        }
        total += connection.execute(
            format!(
                "DELETE FROM {} WHERE \"trade_date\" BETWEEN DATE '{first}' AND DATE '{last}' \
                 AND \"trade_date\" NOT IN (SELECT \"trade_date\" FROM cache_trade_calendar)",
                ident(metadata.table)
            ),
            Parameters::None,
        )?;
    }
    Ok(total)
}

/// Derive trading sessions from the imported source rows. `write` only fetches
/// the recent calendar window, so older imported bars need their dates here or
/// calculation rejects them as absent from the calendar.
fn rebuild_calendar(
    connection: &Connection,
    datasets: &[(&DatasetMetadata, i64)],
) -> Result<usize> {
    let sources = datasets
        .iter()
        .filter(|(metadata, _)| metadata.field("trade_date").is_some())
        .map(|(metadata, _)| format!("SELECT \"trade_date\" FROM {}", ident(metadata.table)))
        .collect::<Vec<_>>();
    ensure!(
        !sources.is_empty(),
        "no imported table has a trade_date column"
    );
    Ok(connection.execute(
        format!(
            "INSERT OR IGNORE INTO cache_trade_calendar \
             SELECT \"trade_date\" FROM ({}) ORDER BY \"trade_date\"",
            sources.join(" UNION ")
        ),
        Parameters::None,
    )?)
}

fn load_postgres(connection: &Connection) -> Result<()> {
    if connection
        .execute("LOAD postgres", Parameters::None)
        .is_ok()
    {
        return Ok(());
    }
    connection
        .execute("INSTALL postgres", Parameters::None)
        .context("install the DuckDB postgres extension (needs network access)")?;
    connection
        .execute("LOAD postgres", Parameters::None)
        .context("load the DuckDB postgres extension")?;
    Ok(())
}

fn insert_keyword(empty: bool, insert_only: bool) -> &'static str {
    if empty {
        "INSERT INTO"
    } else if insert_only {
        "INSERT OR IGNORE INTO"
    } else {
        "INSERT OR REPLACE INTO"
    }
}

struct DateFilter {
    since: Option<NaiveDate>,
    until: Option<NaiveDate>,
    calendar: Option<(NaiveDate, NaiveDate)>,
    skip_off_calendar: bool,
}

impl DateFilter {
    /// Only predicates that the PostgreSQL scanner can push down. Mixing in a
    /// DuckDB calendar subquery makes it fetch the whole date range and filter
    /// locally, which is very slow for wide tables. Off-calendar rows are
    /// pruned from DuckDB after the import instead.
    fn conditions(&self) -> Vec<String> {
        let mut conditions = Vec::new();
        if let Some(since) = self.since {
            conditions.push(format!("\"trade_date\" >= DATE '{since}'"));
        }
        if let Some(until) = self.until {
            conditions.push(format!("\"trade_date\" <= DATE '{until}'"));
        }
        conditions
    }

    fn where_clause(&self) -> String {
        let conditions = self.conditions();
        if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        }
    }
}

fn parse_date(text: &str) -> Result<NaiveDate> {
    ensure!(
        text.len() == 10 && text.as_bytes()[4] == b'-' && text.as_bytes()[7] == b'-',
        "dates must use YYYY-MM-DD: {text}"
    );
    Ok(NaiveDate::parse_from_str(text, "%Y-%m-%d")?)
}

fn latest_cached_date(connection: &Connection) -> Result<Option<NaiveDate>> {
    let mut latest = None;
    for metadata in DATASETS {
        if metadata.field("trade_date").is_none() {
            continue;
        }
        let text = {
            let mut result = connection.query(
                format!(
                    "SELECT max(\"trade_date\")::VARCHAR FROM {}",
                    ident(metadata.table)
                ),
                Parameters::None,
            )?;
            match result.next_chunk()? {
                Some(chunk) => chunk
                    .get_vector_at::<String>(0)?
                    .reader()?
                    .get(0)?
                    .map(str::to_string),
                None => None,
            }
        };
        if let Some(text) = text {
            let date = parse_date(&text)?;
            latest = Some(latest.map_or(date, |current: NaiveDate| current.max(date)));
        }
    }
    Ok(latest)
}

fn calendar_bounds(connection: &Connection) -> Result<Option<(NaiveDate, NaiveDate)>> {
    let mut result = connection.query(
        "SELECT min(\"trade_date\")::VARCHAR, max(\"trade_date\")::VARCHAR FROM cache_trade_calendar",
        Parameters::None,
    )?;
    let Some(chunk) = result.next_chunk()? else {
        return Ok(None);
    };
    let first = chunk
        .get_vector_at::<String>(0)?
        .reader()?
        .get(0)?
        .map(str::to_string);
    let last = chunk
        .get_vector_at::<String>(1)?
        .reader()?
        .get(0)?
        .map(str::to_string);
    match (first, last) {
        (Some(first), Some(last)) => Ok(Some((parse_date(&first)?, parse_date(&last)?))),
        _ => Ok(None),
    }
}

struct ImportPlan<'a> {
    columns: &'a [(String, String)],
    incoming: i64,
    empty: bool,
    filter: &'a DateFilter,
    insert_only: bool,
}

fn import_dataset(
    connection: &Connection,
    metadata: &DatasetMetadata,
    pg_table: &str,
    plan: ImportPlan<'_>,
) -> Result<()> {
    let ImportPlan {
        columns,
        incoming,
        empty,
        filter,
        insert_only,
    } = plan;
    let target = ident(metadata.table);
    // Plain INSERT avoids conflict-index maintenance on a first import.
    let insert = insert_keyword(empty, insert_only);
    let insert_columns = columns
        .iter()
        .map(|(name, _)| ident(name))
        .collect::<Vec<_>>()
        .join(", ");
    let select_columns = columns
        .iter()
        .map(|(name, duckdb_type)| format!("CAST({} AS {duckdb_type})", ident(name)))
        .collect::<Vec<_>>()
        .join(", ");

    if matches!(metadata.write_strategy, WriteStrategy::ReplaceAll { .. }) {
        if insert_only {
            connection.execute(
                format!(
                    "{insert} {target} ({insert_columns}) SELECT {select_columns} FROM {pg_table}"
                ),
                Parameters::None,
            )?;
            return Ok(());
        }
        connection.execute("BEGIN TRANSACTION", Parameters::None)?;
        let result = (|| -> Result<()> {
            connection.execute(format!("DELETE FROM {target}"), Parameters::None)?;
            connection.execute(
                format!(
                    "INSERT INTO {target} ({insert_columns}) SELECT {select_columns} FROM {pg_table}"
                ),
                Parameters::None,
            )?;
            Ok(())
        })();
        if result.is_err() {
            let _ = connection.execute("ROLLBACK", Parameters::None);
        }
        connection.execute("COMMIT", Parameters::None)?;
        return result;
    }

    match metadata.field("trade_date") {
        Some(_) => {
            // Avoid a full-table min/max scan: the PostgreSQL scanner does not
            // push aggregates down, and a 261-column table makes that very slow.
            let first = filter
                .since
                .unwrap_or_else(|| NaiveDate::from_ymd_opt(1990, 1, 1).expect("valid date"));
            let last = filter
                .until
                .unwrap_or_else(|| Utc::now().date_naive() + Duration::days(1));
            if first > last {
                return Ok(());
            }
            // Replace the whole range once, then use plain INSERT. Per-row
            // conflict checks against a large primary-key index are much
            // slower than a range delete followed by append. A range delete
            // still scans the table, so skip it when PostgreSQL has no rows.
            if !insert_only && !empty && incoming > 0 {
                connection.execute(
                    format!(
                        "DELETE FROM {target} \
                         WHERE \"trade_date\" BETWEEN DATE '{first}' AND DATE '{last}'"
                    ),
                    Parameters::None,
                )?;
            }
            let date_insert = if insert_only && !empty {
                "INSERT OR IGNORE INTO"
            } else {
                "INSERT INTO"
            };
            // Monthly chunks keep progress visible during multi-minute loads.
            let mut cursor =
                NaiveDate::from_ymd_opt(first.year(), first.month(), 1).expect("valid month");
            let mut total = 0i64;
            while cursor <= last {
                let (next_year, next_month) = if cursor.month() == 12 {
                    (cursor.year() + 1, 1)
                } else {
                    (cursor.year(), cursor.month() + 1)
                };
                let next = NaiveDate::from_ymd_opt(next_year, next_month, 1).expect("valid month");
                if next > first && cursor <= last {
                    eprintln!("    {cursor}: importing...");
                    let mut conditions = vec![
                        format!("\"trade_date\" >= DATE '{cursor}'"),
                        format!("\"trade_date\" < DATE '{next}'"),
                    ];
                    conditions.extend(filter.conditions());
                    let rows = connection.execute(
                        format!(
                            "{date_insert} {target} ({insert_columns}) \
                             SELECT {select_columns} FROM {pg_table} \
                             WHERE {}",
                            conditions.join(" AND ")
                        ),
                        Parameters::None,
                    )?;
                    total += rows as i64;
                    eprintln!("    {cursor}: {rows} rows (total {total})");
                }
                cursor = next;
            }
            Ok(())
        }
        None => {
            connection.execute(
                format!(
                    "{insert} {target} ({insert_columns}) \
                     SELECT {select_columns} FROM {pg_table}"
                ),
                Parameters::None,
            )?;
            Ok(())
        }
    }
}

fn rebuild_coverage(
    connection: &Connection,
    imported: &[(&DatasetMetadata, i64)],
    filter: Option<&DateFilter>,
) -> Result<usize> {
    let calendar = calendar_bounds(connection)?;
    let range = match (filter, calendar) {
        (Some(filter), Some((calendar_first, calendar_last))) => Some((
            filter.since.unwrap_or(calendar_first),
            filter.until.unwrap_or(calendar_last),
        )),
        (None, Some(bounds)) => Some(bounds),
        _ => None,
    };
    let indexes = configured_indexes()?;
    let mut total = 0usize;
    for (metadata, _) in imported {
        if metadata.field("trade_date").is_none() {
            continue;
        }
        let rows = match range {
            Some((first, last)) if first <= last => {
                if metadata.id == "index_idx_factor_pro" {
                    coverage_for_indexes(connection, first, last, &indexes)?
                } else {
                    coverage_for_dates(connection, metadata.id, metadata.table, first, last)?
                }
            }
            _ => {
                let (code, group) = if metadata.id == "index_idx_factor_pro" {
                    ("\"ts_code\"", ", \"ts_code\"")
                } else {
                    ("''", "")
                };
                connection.execute(
                    format!(
                        "INSERT OR REPLACE INTO cache_fetch_coverage \
                         SELECT {}, \"trade_date\", {code}, count(*) \
                         FROM {} GROUP BY \"trade_date\"{group}",
                        sql_string(metadata.id),
                        ident(metadata.table)
                    ),
                    Parameters::None,
                )?
            }
        };
        total += rows;
    }
    Ok(total)
}

/// Coverage for date-wide requests: every calendar session in the range,
/// including dates whose response was empty.
fn coverage_for_dates(
    connection: &Connection,
    dataset: &str,
    table: &str,
    first: NaiveDate,
    last: NaiveDate,
) -> Result<usize> {
    Ok(connection.execute(
        format!(
            "INSERT OR REPLACE INTO cache_fetch_coverage \
             SELECT {}, cal.\"trade_date\", '', COALESCE(counts.\"row_count\", 0) \
             FROM cache_trade_calendar cal \
             LEFT JOIN ( \
                 SELECT \"trade_date\", count(*) AS \"row_count\" \
                 FROM {} GROUP BY \"trade_date\" \
             ) counts ON counts.\"trade_date\" = cal.\"trade_date\" \
             WHERE cal.\"trade_date\" BETWEEN DATE '{first}' AND DATE '{last}'",
            sql_string(dataset),
            ident(table)
        ),
        Parameters::None,
    )?)
}

/// `index_idx_factor_pro` is fetched per index code. Codes start on different
/// dates (for example 000680.SH in 2025), so the existing rows alone would leave
/// holes in coverage and block the 36-month warmup check.
fn coverage_for_indexes(
    connection: &Connection,
    first: NaiveDate,
    last: NaiveDate,
    indexes: &[String],
) -> Result<usize> {
    let values = indexes
        .iter()
        .map(|code| format!("({})", sql_string(code)))
        .collect::<Vec<_>>()
        .join(", ");
    Ok(connection.execute(
        format!(
            "INSERT OR REPLACE INTO cache_fetch_coverage \
             SELECT 'index_idx_factor_pro', cal.\"trade_date\", codes.\"ts_code\", \
                    COALESCE(counts.\"row_count\", 0) \
             FROM cache_trade_calendar cal \
             CROSS JOIN (VALUES {values}) AS codes(\"ts_code\") \
             LEFT JOIN ( \
                 SELECT \"trade_date\", \"ts_code\", count(*) AS \"row_count\" \
                 FROM index_idx_factor_pro GROUP BY \"trade_date\", \"ts_code\" \
             ) counts \
               ON counts.\"trade_date\" = cal.\"trade_date\" \
              AND counts.\"ts_code\" = codes.\"ts_code\" \
             WHERE cal.\"trade_date\" BETWEEN DATE '{first}' AND DATE '{last}'"
        ),
        Parameters::None,
    )?)
}

fn configured_indexes() -> Result<Vec<String>> {
    let text = match std::env::var("INDEX_LIST_PATH") {
        Ok(path) => std::fs::read_to_string(&path).with_context(|| format!("read {path}"))?,
        Err(_) => include_str!("../config/default-indexes.csv").to_string(),
    };
    parse_indexes(&text)
}

fn parse_indexes(text: &str) -> Result<Vec<String>> {
    let mut codes = BTreeSet::new();
    for (index, line) in text.lines().enumerate().skip(1) {
        let columns = line.split(',').map(str::trim).collect::<Vec<_>>();
        ensure!(
            columns.len() == 4,
            "invalid index-list CSV row {}",
            index + 1
        );
        if columns[3].eq_ignore_ascii_case("true") {
            codes.insert(columns[0].to_string());
        }
    }
    ensure!(!codes.is_empty(), "index list has no enabled codes");
    Ok(codes.into_iter().collect())
}

fn matching_columns(
    metadata: &DatasetMetadata,
    local: &[(String, String)],
    remote: &[(String, String)],
) -> Result<Vec<(String, String)>> {
    let remote_names = remote
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<BTreeSet<_>>();
    let mut columns = Vec::new();
    for (name, duckdb_type) in local {
        if remote_names.contains(name.as_str()) {
            columns.push((name.clone(), duckdb_type.clone()));
        } else if metadata.field(name).is_some_and(|field| !field.nullable) {
            bail!(
                "{}: required column {name} is missing from the PostgreSQL table",
                metadata.id
            );
        }
    }
    ensure!(
        !columns.is_empty(),
        "{}: no shared columns with the PostgreSQL table",
        metadata.id
    );
    Ok(columns)
}

fn describe(connection: &Connection, from: &str) -> Result<Vec<(String, String)>> {
    let mut result =
        connection.query(format!("DESCRIBE SELECT * FROM {from}"), Parameters::None)?;
    let mut columns = Vec::new();
    while let Some(chunk) = result.next_chunk()? {
        let names = chunk.get_vector_at::<String>(0)?.reader()?;
        let types = chunk.get_vector_at::<String>(1)?.reader()?;
        for index in 0..chunk.row_count()? {
            let name = names
                .get(index)?
                .context("missing column name in DESCRIBE output")?;
            let duckdb_type = types
                .get(index)?
                .context("missing column type in DESCRIBE output")?;
            columns.push((name.to_string(), duckdb_type.to_string()));
        }
    }
    Ok(columns)
}

fn table_rows(connection: &Connection, table_ref: &str, where_clause: &str) -> Result<i64> {
    let mut result = connection.query(
        format!("SELECT count(*)::BIGINT FROM {table_ref}{where_clause}"),
        Parameters::None,
    )?;
    let chunk = result
        .next_chunk()?
        .context("PostgreSQL count returned no row")?;
    Ok(*chunk
        .get_vector_at::<i64>(0)?
        .reader()?
        .get(0)?
        .context("PostgreSQL count is NULL")?)
}

fn ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn sql_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// Convert a URL-style DSN into the libpq keyword form used by DuckDB's
/// postgres extension. Plain keyword strings pass through unchanged.
fn normalize_dsn(dsn: &str) -> Result<String> {
    let remainder = dsn
        .strip_prefix("postgresql://")
        .or_else(|| dsn.strip_prefix("postgres://"));
    let Some(remainder) = remainder else {
        return Ok(dsn.to_string());
    };
    let (userinfo, hostpart) = remainder
        .rsplit_once('@')
        .context("PostgreSQL URL is missing '@'")?;
    let (user, password) = userinfo
        .split_once(':')
        .context("PostgreSQL URL is missing a password separator")?;
    let (hostport, database) = hostpart
        .split_once('/')
        .context("PostgreSQL URL is missing a database name")?;
    let (host, port) = match hostport.rsplit_once(':') {
        Some((host, port)) => (host, port),
        None => (hostport, "5432"),
    };
    let mut parts = vec![
        format!("host={}", libpq_value(&percent_decode(host)?)),
        format!("port={}", libpq_value(port)),
        format!("dbname={}", libpq_value(&percent_decode(database)?)),
        format!("user={}", libpq_value(&percent_decode(user)?)),
        format!("password={}", libpq_value(&percent_decode(password)?)),
    ];
    parts.retain(|part| !part.ends_with('='));
    Ok(parts.join(" "))
}

fn libpq_value(value: &str) -> String {
    if value
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '\'' | '\\'))
    {
        format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
    } else {
        value.to_string()
    }
}

fn percent_decode(value: &str) -> Result<String> {
    let bytes = value.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = value
                .get(index + 1..index + 3)
                .context("invalid percent escape in DSN")?;
            result.push(u8::from_str_radix(hex, 16).context("invalid percent escape in DSN")?);
            index += 3;
        } else {
            result.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(result).context("DSN is not valid UTF-8")
}

fn redact_dsn(dsn: &str) -> String {
    if let Some((scheme, rest)) = dsn.split_once("://") {
        if let Some((userinfo, hostpart)) = rest.rsplit_once('@') {
            let user = userinfo.split_once(':').map_or(userinfo, |(user, _)| user);
            return format!("{scheme}://{user}:***@{hostpart}");
        }
    }
    "<keyword DSN>".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_url_dsns_to_libpq_keywords() {
        assert_eq!(
            normalize_dsn("postgresql://user:pa%40ss@host:5432/db").unwrap(),
            "host=host port=5432 dbname=db user=user password=pa@ss"
        );
        assert_eq!(
            normalize_dsn("postgresql://user:pa%20ss@host/db").unwrap(),
            "host=host port=5432 dbname=db user=user password='pa ss'"
        );
        assert_eq!(normalize_dsn("dbname=demo").unwrap(), "dbname=demo");
    }

    #[test]
    fn parses_enabled_index_codes() {
        let text = "ts_code,name,category,enabled\n\
                    000001.SH,A,market,true\n\
                    399001.SZ,B,market,false\n";
        assert_eq!(parse_indexes(text).unwrap(), vec!["000001.SH"]);
    }

    #[test]
    fn insert_only_never_replaces_existing_rows() {
        assert_eq!(insert_keyword(true, true), "INSERT INTO");
        assert_eq!(insert_keyword(false, true), "INSERT OR IGNORE INTO");
        assert_eq!(insert_keyword(false, false), "INSERT OR REPLACE INTO");
    }

    #[test]
    fn date_filter_combines_bounds_and_calendar() {
        let filter = DateFilter {
            since: Some(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()),
            until: Some(NaiveDate::from_ymd_opt(2024, 12, 31).unwrap()),
            calendar: Some((
                NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
                NaiveDate::from_ymd_opt(2024, 12, 30).unwrap(),
            )),
            skip_off_calendar: true,
        };
        let sql = filter.where_clause();
        assert!(sql.contains("DATE '2024-01-01'"));
        assert!(sql.contains("DATE '2024-12-31'"));
        // The calendar is handled by a local prune, not pushed into PostgreSQL.
        assert!(!sql.contains("cache_trade_calendar"));
    }

    #[test]
    fn redacts_the_password_from_url_dsns() {
        assert_eq!(
            redact_dsn("postgresql://user:secret@host:5432/db"),
            "postgresql://user:***@host:5432/db"
        );
        assert_eq!(redact_dsn("dbname=demo user=user"), "<keyword DSN>");
    }
}
