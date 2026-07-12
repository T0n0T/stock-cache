use std::{
    collections::{BTreeMap, HashSet},
    path::PathBuf,
    time::Instant,
};

use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use clap::Parser;
use serde::Serialize;
use tokio_postgres::{Client, NoTls};

use crate::app::write::{parse_cli_date_arg, DateWindow};

use super::{
    constants::CZSC_STRUCTURE_CALC_VERSION,
    model::CzscStructureRow,
    source::{load_source_rows, resolve_calc_dates},
    stock::compute_czsc_rows,
    writer::write_czsc_rows,
};

const DEFAULT_LOOKBACK_TRADING_DAYS: u32 = 252;
const SAMPLE_ROW_LIMIT: usize = 10;

#[derive(Debug, Parser)]
#[command(name = "backfill-czsc-structure")]
#[command(about = "Temporary dedicated backfill for stock_daily_czsc_structure")]
struct CzscBackfillCli {
    #[arg(long)]
    env_file: Option<PathBuf>,

    #[arg(long)]
    start_date: Option<String>,

    #[arg(long)]
    end_date: Option<String>,

    #[arg(long)]
    max_concurrency: Option<u32>,

    #[arg(long)]
    chunk_size: Option<usize>,

    #[arg(long)]
    dry_run: bool,

    #[arg(long)]
    force: bool,
}

#[derive(Debug, Clone)]
struct CzscBackfillOptions {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub max_concurrency: u32,
    pub chunk_size: usize,
    pub dry_run: bool,
    pub force: bool,
}

#[derive(Debug, Serialize)]
struct CzscBackfillSummary {
    pub status: &'static str,
    pub mode: &'static str,
    pub calc_version: &'static str,
    pub force: bool,
    pub requested_date_window: Option<CzscDateWindow>,
    pub pending_date_window: Option<CzscDateWindow>,
    pub target_day_count: usize,
    pub pending_target_day_count: usize,
    pub skipped_complete_day_count: usize,
    pub stock_count: usize,
    pub source_bar_count: usize,
    pub expected_row_count: usize,
    pub written_row_count: usize,
    pub status_counts: BTreeMap<String, usize>,
    pub finished_bi_ratio: Option<f64>,
    pub valid_zs_ratio: Option<f64>,
    pub first_bear_ratio: Option<f64>,
    pub elapsed_seconds: f64,
    pub sample_rows: Vec<CzscSampleRow>,
}

#[derive(Debug, Serialize)]
struct CzscDateWindow {
    pub start_date: String,
    pub end_date: String,
}

#[derive(Debug, Clone, Serialize)]
struct CzscSampleRow {
    pub ts_code: String,
    pub trade_date: String,
    pub status: String,
    pub available_finished_bi_count: i32,
    pub last_bi_direction: Option<String>,
    pub latest_zs_start_date: Option<String>,
    pub recent_peak_date: Option<String>,
    pub post_peak_first_bear_date: Option<String>,
}

fn validate_czsc_backfill_options(options: &CzscBackfillOptions) -> Result<()> {
    if options.start_date.is_some() != options.end_date.is_some() {
        return Err(anyhow!(
            "--start-date and --end-date must be provided together"
        ));
    }
    if options.max_concurrency == 0 {
        return Err(anyhow!("--max-concurrency must be greater than 0"));
    }
    if options.chunk_size == 0 {
        return Err(anyhow!("--chunk-size must be greater than 0"));
    }
    if let (Some(start_date), Some(end_date)) = (&options.start_date, &options.end_date) {
        let start = parse_cli_date_arg("--start-date", start_date)?;
        let end = parse_cli_date_arg("--end-date", end_date)?;
        if start > end {
            return Err(anyhow!("--start-date must be <= --end-date"));
        }
    }
    Ok(())
}

pub async fn run_czsc_backfill_cli() -> Result<()> {
    let cli = CzscBackfillCli::parse();
    let provisional_options = CzscBackfillOptions {
        start_date: cli.start_date,
        end_date: cli.end_date,
        max_concurrency: cli.max_concurrency.unwrap_or(1),
        chunk_size: cli.chunk_size.unwrap_or(1),
        dry_run: cli.dry_run,
        force: cli.force,
    };
    validate_czsc_backfill_options(&provisional_options)?;

    let env_file = crate::app::config::load_env_file(cli.env_file.as_deref())?;
    let config = crate::app::config::Config::from_env_with_options(env_file.as_ref(), false)?;
    let options = CzscBackfillOptions {
        max_concurrency: cli.max_concurrency.unwrap_or(config.max_concurrency),
        chunk_size: cli.chunk_size.or(config.calc_chunk_size).unwrap_or(60),
        ..provisional_options
    };
    validate_czsc_backfill_options(&options)?;
    let summary = run_czsc_backfill(options, &config.postgres_dsn).await?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

async fn run_czsc_backfill(
    options: CzscBackfillOptions,
    postgres_dsn: &str,
) -> Result<CzscBackfillSummary> {
    validate_czsc_backfill_options(&options)?;
    let started = Instant::now();
    eprintln!("[czsc-backfill] connect postgres start");
    let (mut client, connection) = tokio_postgres::connect(postgres_dsn, NoTls).await?;
    let connection_task = tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("postgres connection error: {error}");
        }
    });
    eprintln!("[czsc-backfill] connect postgres done");

    let result = run_connected(&mut client, &options, started).await;
    connection_task.abort();
    result
}

async fn run_connected(
    client: &mut Client,
    options: &CzscBackfillOptions,
    started: Instant,
) -> Result<CzscBackfillSummary> {
    let window = requested_window(options);
    let target_dates = resolve_calc_dates(client, &window).await?;
    let requested_date_window = date_window(&target_dates);
    let complete_dates = if options.force || target_dates.is_empty() {
        HashSet::new()
    } else {
        load_complete_dates(client, &target_dates).await?
    };
    let pending_dates = pending_target_dates(&target_dates, &complete_dates, options.force);
    let pending_date_window = date_window(&pending_dates);
    eprintln!(
        "[czsc-backfill] plan target_days={} pending_days={} skipped_days={} force={} dry_run={} chunk_size={}",
        target_dates.len(),
        pending_dates.len(),
        target_dates.len().saturating_sub(pending_dates.len()),
        options.force,
        options.dry_run,
        options.chunk_size
    );

    let Some(max_trade_date) = pending_dates.last().copied() else {
        return Ok(CzscBackfillSummary {
            status: "no_pending_dates",
            mode: if options.dry_run { "dry_run" } else { "write" },
            calc_version: CZSC_STRUCTURE_CALC_VERSION,
            force: options.force,
            requested_date_window,
            pending_date_window,
            target_day_count: target_dates.len(),
            pending_target_day_count: 0,
            skipped_complete_day_count: target_dates.len(),
            stock_count: 0,
            source_bar_count: 0,
            expected_row_count: 0,
            written_row_count: 0,
            status_counts: CzscRowMetrics::default().status_counts,
            finished_bi_ratio: None,
            valid_zs_ratio: None,
            first_bear_ratio: None,
            elapsed_seconds: started.elapsed().as_secs_f64(),
            sample_rows: Vec::new(),
        });
    };

    eprintln!(
        "[czsc-backfill] load source rows start max_trade_date={}",
        max_trade_date
    );
    let source_rows = load_source_rows(client, max_trade_date).await?;
    eprintln!(
        "[czsc-backfill] load source rows done rows={}",
        source_rows.len()
    );

    let mut metrics = CzscRowMetrics::default();
    let mut written_row_count = 0;
    let chunk_count = pending_dates.len().div_ceil(options.chunk_size);
    for (chunk_index, chunk_dates) in pending_dates.chunks(options.chunk_size).enumerate() {
        eprintln!(
            "[czsc-backfill] chunk {}/{} compute start date_window={}..{} target_days={}",
            chunk_index + 1,
            chunk_count,
            chunk_dates.first().expect("non-empty chunk"),
            chunk_dates.last().expect("non-empty chunk"),
            chunk_dates.len()
        );
        let compute_started = Instant::now();
        let rows = compute_czsc_rows(&source_rows, chunk_dates, options.max_concurrency)?;
        eprintln!(
            "[czsc-backfill] chunk {}/{} compute done rows={} elapsed={:.1}s",
            chunk_index + 1,
            chunk_count,
            rows.len(),
            compute_started.elapsed().as_secs_f64()
        );
        metrics.observe(&rows);
        let write_started = Instant::now();
        let chunk_written = if options.dry_run {
            persist_czsc_rows(None, &rows, true).await?
        } else {
            persist_czsc_rows(Some(client), &rows, false).await?
        };
        written_row_count += chunk_written;
        eprintln!(
            "[czsc-backfill] chunk {}/{} write done written_rows={} dry_run={} elapsed={:.1}s",
            chunk_index + 1,
            chunk_count,
            chunk_written,
            options.dry_run,
            write_started.elapsed().as_secs_f64()
        );
    }

    let finished_bi_ratio = metrics.finished_bi_ratio();
    let valid_zs_ratio = metrics.valid_zs_ratio();
    let first_bear_ratio = metrics.first_bear_ratio();
    Ok(CzscBackfillSummary {
        status: "ok",
        mode: if options.dry_run { "dry_run" } else { "write" },
        calc_version: CZSC_STRUCTURE_CALC_VERSION,
        force: options.force,
        requested_date_window,
        pending_date_window,
        target_day_count: target_dates.len(),
        pending_target_day_count: pending_dates.len(),
        skipped_complete_day_count: target_dates.len().saturating_sub(pending_dates.len()),
        stock_count: metrics.stock_codes.len(),
        source_bar_count: source_rows.len(),
        expected_row_count: metrics.row_count,
        written_row_count,
        status_counts: metrics.status_counts,
        finished_bi_ratio,
        valid_zs_ratio,
        first_bear_ratio,
        elapsed_seconds: started.elapsed().as_secs_f64(),
        sample_rows: metrics.sample_rows,
    })
}

fn requested_window(options: &CzscBackfillOptions) -> DateWindow {
    match (&options.start_date, &options.end_date) {
        (Some(start_date), Some(end_date)) => DateWindow::Explicit {
            start_date: start_date.clone(),
            end_date: end_date.clone(),
        },
        _ => DateWindow::DefaultLookbackTradingDays(DEFAULT_LOOKBACK_TRADING_DAYS),
    }
}

fn pending_target_dates(
    target_dates: &[NaiveDate],
    complete_dates: &HashSet<NaiveDate>,
    force: bool,
) -> Vec<NaiveDate> {
    target_dates
        .iter()
        .copied()
        .filter(|date| force || !complete_dates.contains(date))
        .collect()
}

async fn load_complete_dates(
    client: &Client,
    target_dates: &[NaiveDate],
) -> Result<HashSet<NaiveDate>> {
    let Some(start_date) = target_dates.first() else {
        return Ok(HashSet::new());
    };
    let end_date = target_dates.last().expect("checked non-empty");
    let rows = client
        .query(
            r#"
            select source."trade_date"
            from "stock_stk_factor_pro" source
            left join "stock_daily_czsc_structure" czsc
              on czsc."ts_code" = source."ts_code"
             and czsc."trade_date" = source."trade_date"
             and czsc."calc_version" = $3
            where source."trade_date" between $1 and $2
            group by source."trade_date"
            having count(*) = count(czsc."ts_code")
            order by source."trade_date"
            "#,
            &[start_date, end_date, &CZSC_STRUCTURE_CALC_VERSION],
        )
        .await?;
    Ok(rows.into_iter().map(|row| row.get(0)).collect())
}

async fn persist_czsc_rows(
    client: Option<&mut Client>,
    rows: &[CzscStructureRow],
    dry_run: bool,
) -> Result<usize> {
    if dry_run {
        return Ok(0);
    }
    let client = client.ok_or_else(|| anyhow!("database client is required outside dry-run"))?;
    write_czsc_rows(client, rows).await
}

struct CzscRowMetrics {
    row_count: usize,
    stock_codes: HashSet<String>,
    status_counts: BTreeMap<String, usize>,
    finished_bi_count: usize,
    valid_zs_count: usize,
    first_bear_count: usize,
    sample_rows: Vec<CzscSampleRow>,
}

impl Default for CzscRowMetrics {
    fn default() -> Self {
        Self {
            row_count: 0,
            stock_codes: HashSet::new(),
            status_counts: BTreeMap::from([
                ("invalid_source_bar".to_string(), 0),
                ("no_finished_bi".to_string(), 0),
                ("insufficient_bis_for_zs".to_string(), 0),
                ("no_valid_zs".to_string(), 0),
                ("ok".to_string(), 0),
            ]),
            finished_bi_count: 0,
            valid_zs_count: 0,
            first_bear_count: 0,
            sample_rows: Vec::new(),
        }
    }
}

impl CzscRowMetrics {
    fn observe(&mut self, rows: &[CzscStructureRow]) {
        self.row_count += rows.len();
        for row in rows {
            self.stock_codes.insert(row.ts_code.clone());
            *self.status_counts.entry(row.status.clone()).or_default() += 1;
            self.finished_bi_count += usize::from(row.available_finished_bi_count > 0);
            self.valid_zs_count += usize::from(row.latest_zs_start_date.is_some());
            self.first_bear_count += usize::from(row.post_peak_first_bear_date.is_some());
            self.sample_rows.push(CzscSampleRow::from(row));
        }
        self.sample_rows.sort_by(|left, right| {
            left.trade_date
                .cmp(&right.trade_date)
                .then_with(|| left.ts_code.cmp(&right.ts_code))
        });
        self.sample_rows.truncate(SAMPLE_ROW_LIMIT);
    }

    fn finished_bi_ratio(&self) -> Option<f64> {
        ratio(self.finished_bi_count, self.row_count)
    }

    fn valid_zs_ratio(&self) -> Option<f64> {
        ratio(self.valid_zs_count, self.row_count)
    }

    fn first_bear_ratio(&self) -> Option<f64> {
        ratio(self.first_bear_count, self.row_count)
    }
}

impl From<&CzscStructureRow> for CzscSampleRow {
    fn from(row: &CzscStructureRow) -> Self {
        Self {
            ts_code: row.ts_code.clone(),
            trade_date: row.trade_date.to_string(),
            status: row.status.clone(),
            available_finished_bi_count: row.available_finished_bi_count,
            last_bi_direction: row.last_bi_direction.clone(),
            latest_zs_start_date: row.latest_zs_start_date.map(|date| date.to_string()),
            recent_peak_date: row.recent_peak_date.map(|date| date.to_string()),
            post_peak_first_bear_date: row.post_peak_first_bear_date.map(|date| date.to_string()),
        }
    }
}

#[cfg(test)]
fn summarize_czsc_rows(rows: &[CzscStructureRow]) -> CzscRowMetrics {
    let mut summary = CzscRowMetrics::default();
    summary.observe(rows);
    summary
}

fn ratio(count: usize, total: usize) -> Option<f64> {
    (total > 0).then_some(count as f64 / total as f64)
}

fn date_window(dates: &[NaiveDate]) -> Option<CzscDateWindow> {
    Some(CzscDateWindow {
        start_date: dates.first()?.to_string(),
        end_date: dates.last()?.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::model::CzscStructureRow;
    use chrono::NaiveDate;

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 1, day).expect("test date")
    }

    fn row(ts_code: &str, day: u32, status: &str) -> CzscStructureRow {
        CzscStructureRow {
            ts_code: ts_code.to_string(),
            trade_date: date(day),
            source_bar_valid: true,
            status: status.to_string(),
            available_finished_bi_count: 0,
            recent_bi_count: 0,
            last_bi_direction: None,
            last_bi_start_date: None,
            last_bi_end_date: None,
            last_bi_high: None,
            last_bi_low: None,
            last_bi_mid: None,
            latest_zs_start_date: None,
            latest_zs_end_date: None,
            latest_zs_bi_count: None,
            latest_zs_zg: None,
            latest_zs_zd: None,
            latest_zs_zz: None,
            recent_peak_date: None,
            recent_peak_high: None,
            post_peak_first_bear_date: None,
            post_peak_first_bear_open: None,
            post_peak_first_bear_high: None,
            post_peak_first_bear_low: None,
            post_peak_first_bear_close: None,
            post_peak_first_bear_missing: None,
        }
    }

    #[tokio::test]
    async fn dry_run_persistence_never_requires_a_database_client() {
        let rows = vec![row("000001.SZ", 2, "no_finished_bi")];

        let written = persist_czsc_rows(None, &rows, true)
            .await
            .expect("dry-run must not call the writer");

        assert_eq!(written, 0);
    }

    #[test]
    fn complete_dates_are_skipped_unless_force_is_explicit() {
        let target_dates = vec![date(1), date(2), date(3)];
        let complete_dates = HashSet::from([date(2)]);

        assert_eq!(
            pending_target_dates(&target_dates, &complete_dates, false),
            vec![date(1), date(3)]
        );
        assert_eq!(
            pending_target_dates(&target_dates, &complete_dates, true),
            target_dates
        );
    }

    #[test]
    fn row_summary_reports_distributions_ratios_and_stable_samples() {
        let mut rows = vec![
            row("000002.SZ", 2, "no_finished_bi"),
            row("000001.SZ", 2, "ok"),
            row("000001.SZ", 1, "invalid_source_bar"),
        ];
        rows[1].available_finished_bi_count = 3;
        rows[1].latest_zs_start_date = Some(date(1));
        rows[1].post_peak_first_bear_date = Some(date(2));

        let summary = summarize_czsc_rows(&rows);

        assert_eq!(summary.status_counts["invalid_source_bar"], 1);
        assert_eq!(summary.status_counts["no_finished_bi"], 1);
        assert_eq!(summary.status_counts["ok"], 1);
        assert_eq!(summary.finished_bi_ratio(), Some(1.0 / 3.0));
        assert_eq!(summary.valid_zs_ratio(), Some(1.0 / 3.0));
        assert_eq!(summary.first_bear_ratio(), Some(1.0 / 3.0));
        assert_eq!(
            summary
                .sample_rows
                .iter()
                .map(|row| (row.ts_code.as_str(), row.trade_date.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("000001.SZ", "2024-01-01"),
                ("000001.SZ", "2024-01-02"),
                ("000002.SZ", "2024-01-02"),
            ]
        );
    }
}
