use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    fs,
    sync::Arc,
    time::{Duration as StdDuration, Instant},
};

use anyhow::{anyhow, Context, Result};
use chrono::{Datelike, Duration as ChronoDuration, NaiveDate};
use serde::Serialize;
use serde_json::json;
use tokio::{sync::Mutex, time::sleep};
use tokio_postgres::NoTls;

use crate::{
    app::{config::Config, write::DateWindow},
    metadata::tushare_fields::{dataset, DatasetMetadata},
    source::{
        tushare_worker::{string_param, worker_params, PythonWorker},
        writer::{parse_date_text, replace_refresh_group, write_dataset, SourceRow},
    },
};

const DAILY_CACHE_DATASETS: &[&str] = &[
    "stock_stk_factor_pro",
    "stock_moneyflow_ths",
    "stock_limit_list_ths",
    "index_limit_cpt_list",
];
const THS_MEMBER_REQUEST_DELAY_MS: u64 = 150;
const CODE_REQUEST_DELAY_MS: u64 = 150;

#[derive(Debug, Serialize)]
pub struct CacheSummary {
    pub status: &'static str,
    pub mode: &'static str,
    pub refresh: bool,
    pub source_row_counts: BTreeMap<String, usize>,
    pub message: &'static str,
}

pub async fn run_cache(
    window: &DateWindow,
    refresh: bool,
    config: &Config,
) -> Result<CacheSummary> {
    let dates = resolve_cache_dates(window)?;
    run_cache_for_trade_dates(&dates, refresh, config).await
}

pub async fn run_cache_for_trade_dates(
    dates: &[NaiveDate],
    refresh: bool,
    config: &Config,
) -> Result<CacheSummary> {
    let (mut client, connection) = tokio_postgres::connect(&config.postgres_dsn, NoTls).await?;
    let connection_task = tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("postgres connection error: {error}");
        }
    });

    let mut row_counts = BTreeMap::new();
    let total_dates = dates.len();

    let index_codes = if dates.is_empty() {
        Vec::new()
    } else {
        enabled_index_codes(&config.index_list_path)?
    };
    let target_plan = load_target_cache_plan(&client, dates, &index_codes).await?;

    if !refresh && target_plan.is_complete() {
        if !dates.is_empty() {
            eprintln!(
                "[cache] source coverage complete trade_days={}; skip target cache",
                dates.len()
            );
        }
        connection_task.abort();
        return Ok(CacheSummary {
            status: "success",
            mode: "cache",
            refresh,
            source_row_counts: row_counts,
            message: "source cache write completed",
        });
    }

    let mut command = PythonWorker::command_from_env();
    command.env("TUSHARE_TOKEN", &config.tushare_token);
    let mut worker = PythonWorker::spawn(command).await?;

    if refresh {
        let started = Instant::now();
        eprintln!("[cache] refresh start");
        let stock_basic = required_dataset("stock_basic")?;
        let index_ths = required_dataset("index_ths")?;
        let index_ths_member = required_dataset("index_ths_member")?;
        let stock_basic_rows = fetch(
            &mut worker,
            "refresh-stock-basic",
            stock_basic,
            &BTreeMap::new(),
        )
        .await?;
        let index_ths_rows = fetch(
            &mut worker,
            "refresh-index-ths",
            index_ths,
            &BTreeMap::new(),
        )
        .await?;
        let mut index_ths_member_rows = Vec::new();
        let total_index_ths_rows = index_ths_rows.len();
        for (member_index, row) in index_ths_rows.iter().enumerate() {
            let Some(ts_code) = row.get("ts_code").and_then(|value| value.as_str()) else {
                continue;
            };
            if member_index == 0 || (member_index + 1) % 25 == 0 {
                eprintln!(
                    "[cache] refresh index_ths_member {}/{} {}",
                    member_index + 1,
                    total_index_ths_rows,
                    ts_code
                );
            }
            let params = worker_params(&[("ts_code", string_param(ts_code))]);
            let mut rows = fetch(
                &mut worker,
                &format!("refresh-index-ths-member-{ts_code}"),
                index_ths_member,
                &params,
            )
            .await?;
            index_ths_member_rows.append(&mut rows);
            sleep(StdDuration::from_millis(THS_MEMBER_REQUEST_DELAY_MS)).await;
        }
        replace_refresh_group(
            &mut client,
            &[
                (stock_basic, &stock_basic_rows),
                (index_ths, &index_ths_rows),
                (index_ths_member, &index_ths_member_rows),
            ],
        )
        .await?;
        row_counts.insert(stock_basic.id.to_string(), stock_basic_rows.len());
        row_counts.insert(index_ths.id.to_string(), index_ths_rows.len());
        row_counts.insert(index_ths_member.id.to_string(), index_ths_member_rows.len());
        eprintln!(
            "[cache] refresh done stock_basic={} index_ths={} index_ths_member={} elapsed={:.1}s",
            stock_basic_rows.len(),
            index_ths_rows.len(),
            index_ths_member_rows.len(),
            started.elapsed().as_secs_f64()
        );
    }

    if target_plan.is_complete() {
        if !dates.is_empty() {
            eprintln!(
                "[cache] source coverage complete trade_days={}; skip target cache",
                dates.len()
            );
        }
        connection_task.abort();
        return Ok(CacheSummary {
            status: "success",
            mode: "cache",
            refresh,
            source_row_counts: row_counts,
            message: "source cache write completed",
        });
    }

    eprintln!(
        "[cache] source coverage missing trade_days={} work_days={} daily_requests={} index_code_requests={} ths_daily_days={} moneyflow_days={}",
        target_plan.total_dates,
        target_plan.work_items.len(),
        target_plan.daily_dataset_request_count(),
        target_plan.index_code_request_count(),
        target_plan.index_ths_daily_day_count(),
        target_plan.index_moneyflow_day_count()
    );

    for plan in &target_plan.work_items {
        let day_started = Instant::now();
        let trade_date_text = plan.trade_date.format("%Y%m%d").to_string();
        eprintln!(
            "[cache] day {}/{} {} start",
            plan.date_index + 1,
            total_dates,
            trade_date_text
        );
        for dataset_id in &plan.missing_daily_datasets {
            let dataset = required_dataset(dataset_id)?;
            let params = worker_params(&[("trade_date", string_param(&trade_date_text))]);
            let rows = fetch(
                &mut worker,
                &format!("{dataset_id}-{trade_date_text}"),
                dataset,
                &params,
            )
            .await?;
            let delete_date = if *dataset_id == "index_limit_cpt_list" {
                Some(plan.trade_date)
            } else {
                None
            };
            write_dataset(&mut client, dataset, &rows, delete_date).await?;
            *row_counts.entry((*dataset_id).to_string()).or_insert(0) += rows.len();
        }

        if !plan.missing_index_codes.is_empty() {
            let index_dataset = required_dataset("index_idx_factor_pro")?;
            for ts_code in &plan.missing_index_codes {
                let params = worker_params(&[
                    ("ts_code", string_param(ts_code)),
                    ("start_date", string_param(&trade_date_text)),
                    ("end_date", string_param(&trade_date_text)),
                ]);
                let rows = fetch(
                    &mut worker,
                    &format!("index_idx_factor_pro-{ts_code}-{trade_date_text}"),
                    index_dataset,
                    &params,
                )
                .await?;
                write_dataset(&mut client, index_dataset, &rows, None).await?;
                *row_counts.entry(index_dataset.id.to_string()).or_insert(0) += rows.len();
                sleep(StdDuration::from_millis(CODE_REQUEST_DELAY_MS)).await;
            }
        }

        if plan.missing_index_ths_daily {
            let index_ths_daily = required_dataset("index_ths_daily")?;
            let params = trade_date_params(&trade_date_text);
            let rows = fetch(
                &mut worker,
                &format!("index_ths_daily-{trade_date_text}"),
                index_ths_daily,
                &params,
            )
            .await?;
            write_dataset(&mut client, index_ths_daily, &rows, None).await?;
            *row_counts
                .entry(index_ths_daily.id.to_string())
                .or_insert(0) += rows.len();
        }

        if plan.missing_index_moneyflow {
            let moneyflow_cnt = required_dataset("index_moneyflow_cnt_ths")?;
            let params = worker_params(&[("trade_date", string_param(&trade_date_text))]);
            let mut rows = fetch(
                &mut worker,
                &format!("index_moneyflow_cnt_ths-{trade_date_text}"),
                moneyflow_cnt,
                &params,
            )
            .await?;
            let skipped_rows = retain_rows_with_non_empty_field(&mut rows, "ts_code");
            if skipped_rows > 0 {
                eprintln!(
                    "[cache] index_moneyflow_cnt_ths {trade_date_text} skip rows missing ts_code count={skipped_rows}"
                );
            }
            write_dataset(&mut client, moneyflow_cnt, &rows, None).await?;
            *row_counts.entry(moneyflow_cnt.id.to_string()).or_insert(0) += rows.len();
        }
        eprintln!(
            "[cache] day {}/{} {} done elapsed={:.1}s",
            plan.date_index + 1,
            total_dates,
            trade_date_text,
            day_started.elapsed().as_secs_f64()
        );
    }

    connection_task.abort();
    Ok(CacheSummary {
        status: "success",
        mode: "cache",
        refresh,
        source_row_counts: row_counts,
        message: "source cache write completed",
    })
}

pub async fn run_calc_source_cache_for_trade_dates(
    dates: &[NaiveDate],
    config: &Config,
) -> Result<CacheSummary> {
    run_calc_source_cache_for_trade_dates_with_concurrency(
        dates,
        config,
        config.cache_warmup_concurrency,
    )
    .await
}

pub async fn run_calc_source_cache_for_trade_dates_with_concurrency(
    dates: &[NaiveDate],
    config: &Config,
    concurrency: usize,
) -> Result<CacheSummary> {
    let index_codes = enabled_index_codes(&config.index_list_path)?;
    let plan = load_warmup_cache_plan(dates, &index_codes, config).await?;
    if plan.is_complete() {
        eprintln!(
            "[warmup] source coverage complete trade_days={}; skip cache warmup",
            plan.total_dates
        );
        return Ok(CacheSummary {
            status: "success",
            mode: "cache",
            refresh: false,
            source_row_counts: BTreeMap::new(),
            message: "calc source cache warmup completed",
        });
    }

    eprintln!(
        "[warmup] source coverage missing trade_days={} work_days={} stock_days={} index_code_requests={} ths_daily_days={}",
        plan.total_dates,
        plan.work_items.len(),
        plan.stock_factor_day_count(),
        plan.index_code_request_count(),
        plan.index_ths_daily_day_count()
    );
    let worker_count = concurrency.max(1).min(plan.work_items.len().max(1));
    if worker_count <= 1 {
        return run_calc_source_cache_worker(&plan.work_items, config, 1, 1, plan.total_dates)
            .await;
    }

    eprintln!(
        "[warmup] concurrent cache start workers={} trade_days={}",
        worker_count,
        plan.work_items.len()
    );
    let queue = Arc::new(Mutex::new(
        plan.work_items.iter().cloned().collect::<VecDeque<_>>(),
    ));
    let mut handles = Vec::new();
    for worker_index in 0..worker_count {
        let queue = Arc::clone(&queue);
        let config = config.clone();
        let total_dates = plan.total_dates;
        handles.push(tokio::spawn(async move {
            run_calc_source_cache_queue_worker(
                queue,
                &config,
                worker_index + 1,
                worker_count,
                total_dates,
            )
            .await
        }));
    }

    let mut row_counts = BTreeMap::new();
    for handle in handles {
        let summary = handle.await??;
        merge_counts(&mut row_counts, &summary.source_row_counts);
    }

    Ok(CacheSummary {
        status: "success",
        mode: "cache",
        refresh: false,
        source_row_counts: row_counts,
        message: "calc source cache warmup completed",
    })
}

async fn load_warmup_cache_plan(
    dates: &[NaiveDate],
    index_codes: &[String],
    config: &Config,
) -> Result<WarmupCachePlan> {
    let stock_dataset = required_dataset("stock_stk_factor_pro")?;
    let index_dataset = required_dataset("index_idx_factor_pro")?;
    let index_ths_daily = required_dataset("index_ths_daily")?;
    let (client, connection) = tokio_postgres::connect(&config.postgres_dsn, NoTls).await?;
    let connection_task = tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("postgres connection error: {error}");
        }
    });

    let result = load_warmup_existing_coverage(
        &client,
        dates,
        stock_dataset,
        index_dataset,
        index_ths_daily,
    )
    .await
    .map(|coverage| warmup_cache_plan_from_coverage(dates, index_codes, &coverage));

    connection_task.abort();
    result
}

async fn load_warmup_existing_coverage(
    client: &tokio_postgres::Client,
    dates: &[NaiveDate],
    stock_dataset: &DatasetMetadata,
    index_dataset: &DatasetMetadata,
    index_ths_daily: &DatasetMetadata,
) -> Result<WarmupExistingCoverage> {
    Ok(WarmupExistingCoverage {
        stock_factor_dates: existing_trade_dates_in_window(client, stock_dataset.id, dates).await?,
        index_factor_dates: existing_trade_dates_in_window(client, index_dataset.id, dates).await?,
        index_ths_daily_dates: existing_trade_dates_in_window(client, index_ths_daily.id, dates)
            .await?,
    })
}

async fn load_target_cache_plan(
    client: &tokio_postgres::Client,
    dates: &[NaiveDate],
    index_codes: &[String],
) -> Result<TargetCachePlan> {
    let coverage = load_target_existing_coverage(client, dates).await?;
    Ok(target_cache_plan_from_coverage(
        dates,
        index_codes,
        &coverage,
    ))
}

async fn load_target_existing_coverage(
    client: &tokio_postgres::Client,
    dates: &[NaiveDate],
) -> Result<TargetExistingCoverage> {
    let mut coverage = TargetExistingCoverage::default();
    for dataset_id in DAILY_CACHE_DATASETS {
        let dataset = required_dataset(dataset_id)?;
        coverage.daily_dataset_dates.insert(
            dataset.id,
            existing_trade_dates_in_window(client, dataset.id, dates).await?,
        );
    }

    let index_dataset = required_dataset("index_idx_factor_pro")?;
    coverage.index_factor_codes_by_date =
        existing_ts_codes_by_date_in_window(client, index_dataset.id, dates).await?;

    let index_ths_daily = required_dataset("index_ths_daily")?;
    coverage.index_ths_daily_dates =
        existing_trade_dates_in_window(client, index_ths_daily.id, dates).await?;

    let moneyflow_cnt = required_dataset("index_moneyflow_cnt_ths")?;
    coverage.index_moneyflow_dates =
        existing_trade_dates_in_window(client, moneyflow_cnt.id, dates).await?;

    Ok(coverage)
}

async fn run_calc_source_cache_queue_worker(
    queue: Arc<Mutex<VecDeque<WarmupCacheTradeDatePlan>>>,
    config: &Config,
    worker_index: usize,
    worker_count: usize,
    total_dates: usize,
) -> Result<CacheSummary> {
    let (mut client, connection) = tokio_postgres::connect(&config.postgres_dsn, NoTls).await?;
    let connection_task = tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("postgres connection error: {error}");
        }
    });

    let mut command = PythonWorker::command_from_env();
    command.env("TUSHARE_TOKEN", &config.tushare_token);
    let mut worker = PythonWorker::spawn(command).await?;
    let stock_dataset = required_dataset("stock_stk_factor_pro")?;
    let index_dataset = required_dataset("index_idx_factor_pro")?;
    let index_ths_daily = required_dataset("index_ths_daily")?;
    let mut row_counts = BTreeMap::new();

    loop {
        let Some(plan) = queue.lock().await.pop_front() else {
            break;
        };
        warmup_cache_trade_date(WarmupCacheTradeDate {
            client: &mut client,
            worker: &mut worker,
            stock_dataset,
            index_dataset,
            index_ths_daily,
            row_counts: &mut row_counts,
            plan: &plan,
            total_dates,
            worker_index,
            worker_count,
        })
        .await?;
    }

    connection_task.abort();
    Ok(CacheSummary {
        status: "success",
        mode: "cache",
        refresh: false,
        source_row_counts: row_counts,
        message: "calc source cache warmup completed",
    })
}

async fn run_calc_source_cache_worker(
    plans: &[WarmupCacheTradeDatePlan],
    config: &Config,
    worker_index: usize,
    worker_count: usize,
    total_dates: usize,
) -> Result<CacheSummary> {
    let (mut client, connection) = tokio_postgres::connect(&config.postgres_dsn, NoTls).await?;
    let connection_task = tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("postgres connection error: {error}");
        }
    });

    let mut command = PythonWorker::command_from_env();
    command.env("TUSHARE_TOKEN", &config.tushare_token);
    let mut worker = PythonWorker::spawn(command).await?;
    let mut row_counts = BTreeMap::new();
    let stock_dataset = required_dataset("stock_stk_factor_pro")?;
    let index_dataset = required_dataset("index_idx_factor_pro")?;
    let index_ths_daily = required_dataset("index_ths_daily")?;

    for plan in plans {
        warmup_cache_trade_date(WarmupCacheTradeDate {
            client: &mut client,
            worker: &mut worker,
            stock_dataset,
            index_dataset,
            index_ths_daily,
            row_counts: &mut row_counts,
            plan,
            total_dates,
            worker_index,
            worker_count,
        })
        .await?;
    }

    connection_task.abort();
    Ok(CacheSummary {
        status: "success",
        mode: "cache",
        refresh: false,
        source_row_counts: row_counts,
        message: "calc source cache warmup completed",
    })
}

struct WarmupCacheTradeDate<'a> {
    client: &'a mut tokio_postgres::Client,
    worker: &'a mut PythonWorker,
    stock_dataset: &'static DatasetMetadata,
    index_dataset: &'static DatasetMetadata,
    index_ths_daily: &'static DatasetMetadata,
    row_counts: &'a mut BTreeMap<String, usize>,
    plan: &'a WarmupCacheTradeDatePlan,
    total_dates: usize,
    worker_index: usize,
    worker_count: usize,
}

async fn warmup_cache_trade_date(args: WarmupCacheTradeDate<'_>) -> Result<()> {
    let day_started = Instant::now();
    let trade_date_text = args.plan.trade_date.format("%Y%m%d").to_string();
    let worker_label = format!("{}/{}", args.worker_index, args.worker_count);
    if args.plan.missing_stock_factor {
        eprintln!(
            "[warmup] worker {} stock_stk_factor_pro day {}/{} {} start",
            worker_label,
            args.plan.date_index + 1,
            args.total_dates,
            trade_date_text
        );
        let params = worker_params(&[("trade_date", string_param(&trade_date_text))]);
        let rows = fetch(
            args.worker,
            &format!("stock_stk_factor_pro-warmup-{trade_date_text}"),
            args.stock_dataset,
            &params,
        )
        .await?;
        write_dataset(args.client, args.stock_dataset, &rows, None).await?;
        *args
            .row_counts
            .entry(args.stock_dataset.id.to_string())
            .or_insert(0) += rows.len();
        eprintln!(
            "[warmup] worker {} stock_stk_factor_pro day {}/{} {} done rows={} elapsed={:.1}s",
            worker_label,
            args.plan.date_index + 1,
            args.total_dates,
            trade_date_text,
            rows.len(),
            day_started.elapsed().as_secs_f64()
        );
    }

    for ts_code in &args.plan.missing_index_codes {
        let params = worker_params(&[
            ("ts_code", string_param(ts_code)),
            ("start_date", string_param(&trade_date_text)),
            ("end_date", string_param(&trade_date_text)),
        ]);
        let rows = fetch(
            args.worker,
            &format!("index_idx_factor_pro-warmup-{ts_code}-{trade_date_text}"),
            args.index_dataset,
            &params,
        )
        .await?;
        write_dataset(args.client, args.index_dataset, &rows, None).await?;
        *args
            .row_counts
            .entry(args.index_dataset.id.to_string())
            .or_insert(0) += rows.len();
        sleep(StdDuration::from_millis(CODE_REQUEST_DELAY_MS)).await;
    }

    if args.plan.missing_index_ths_daily {
        let params = trade_date_params(&trade_date_text);
        let rows = fetch(
            args.worker,
            &format!("index_ths_daily-warmup-{trade_date_text}"),
            args.index_ths_daily,
            &params,
        )
        .await?;
        write_dataset(args.client, args.index_ths_daily, &rows, None).await?;
        *args
            .row_counts
            .entry(args.index_ths_daily.id.to_string())
            .or_insert(0) += rows.len();
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WarmupCacheTradeDatePlan {
    date_index: usize,
    trade_date: NaiveDate,
    missing_stock_factor: bool,
    missing_index_codes: Vec<String>,
    missing_index_ths_daily: bool,
}

impl WarmupCacheTradeDatePlan {
    fn has_work(&self) -> bool {
        self.missing_stock_factor
            || !self.missing_index_codes.is_empty()
            || self.missing_index_ths_daily
    }
}

#[derive(Debug, Eq, PartialEq)]
struct WarmupCachePlan {
    total_dates: usize,
    work_items: Vec<WarmupCacheTradeDatePlan>,
}

impl WarmupCachePlan {
    fn is_complete(&self) -> bool {
        self.work_items.is_empty()
    }

    fn stock_factor_day_count(&self) -> usize {
        self.work_items
            .iter()
            .filter(|item| item.missing_stock_factor)
            .count()
    }

    fn index_code_request_count(&self) -> usize {
        self.work_items
            .iter()
            .map(|item| item.missing_index_codes.len())
            .sum()
    }

    fn index_ths_daily_day_count(&self) -> usize {
        self.work_items
            .iter()
            .filter(|item| item.missing_index_ths_daily)
            .count()
    }
}

#[derive(Debug, Default)]
struct WarmupExistingCoverage {
    stock_factor_dates: HashSet<NaiveDate>,
    index_factor_dates: HashSet<NaiveDate>,
    index_ths_daily_dates: HashSet<NaiveDate>,
}

fn warmup_cache_plan_from_coverage(
    dates: &[NaiveDate],
    index_codes: &[String],
    coverage: &WarmupExistingCoverage,
) -> WarmupCachePlan {
    let work_items = dates
        .iter()
        .copied()
        .enumerate()
        .map(|(date_index, trade_date)| WarmupCacheTradeDatePlan {
            date_index,
            trade_date,
            missing_stock_factor: !coverage.stock_factor_dates.contains(&trade_date),
            missing_index_codes: if coverage.index_factor_dates.contains(&trade_date) {
                Vec::new()
            } else {
                index_codes.to_vec()
            },
            missing_index_ths_daily: !coverage.index_ths_daily_dates.contains(&trade_date),
        })
        .filter(WarmupCacheTradeDatePlan::has_work)
        .collect();

    WarmupCachePlan {
        total_dates: dates.len(),
        work_items,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TargetCacheTradeDatePlan {
    date_index: usize,
    trade_date: NaiveDate,
    missing_daily_datasets: Vec<&'static str>,
    missing_index_codes: Vec<String>,
    missing_index_ths_daily: bool,
    missing_index_moneyflow: bool,
}

impl TargetCacheTradeDatePlan {
    fn has_work(&self) -> bool {
        !self.missing_daily_datasets.is_empty()
            || !self.missing_index_codes.is_empty()
            || self.missing_index_ths_daily
            || self.missing_index_moneyflow
    }
}

#[derive(Debug, Eq, PartialEq)]
struct TargetCachePlan {
    total_dates: usize,
    work_items: Vec<TargetCacheTradeDatePlan>,
}

impl TargetCachePlan {
    fn is_complete(&self) -> bool {
        self.work_items.is_empty()
    }

    fn daily_dataset_request_count(&self) -> usize {
        self.work_items
            .iter()
            .map(|item| item.missing_daily_datasets.len())
            .sum()
    }

    fn index_code_request_count(&self) -> usize {
        self.work_items
            .iter()
            .map(|item| item.missing_index_codes.len())
            .sum()
    }

    fn index_ths_daily_day_count(&self) -> usize {
        self.work_items
            .iter()
            .filter(|item| item.missing_index_ths_daily)
            .count()
    }

    fn index_moneyflow_day_count(&self) -> usize {
        self.work_items
            .iter()
            .filter(|item| item.missing_index_moneyflow)
            .count()
    }
}

#[derive(Debug, Default)]
struct TargetExistingCoverage {
    daily_dataset_dates: BTreeMap<&'static str, HashSet<NaiveDate>>,
    index_factor_codes_by_date: BTreeMap<NaiveDate, HashSet<String>>,
    index_ths_daily_dates: HashSet<NaiveDate>,
    index_moneyflow_dates: HashSet<NaiveDate>,
}

fn target_cache_plan_from_coverage(
    dates: &[NaiveDate],
    index_codes: &[String],
    coverage: &TargetExistingCoverage,
) -> TargetCachePlan {
    let work_items = dates
        .iter()
        .copied()
        .enumerate()
        .map(|(date_index, trade_date)| {
            let existing_index_codes = coverage
                .index_factor_codes_by_date
                .get(&trade_date)
                .cloned()
                .unwrap_or_default();
            TargetCacheTradeDatePlan {
                date_index,
                trade_date,
                missing_daily_datasets: DAILY_CACHE_DATASETS
                    .iter()
                    .copied()
                    .filter(|dataset_id| {
                        !coverage
                            .daily_dataset_dates
                            .get(dataset_id)
                            .is_some_and(|covered_dates| covered_dates.contains(&trade_date))
                    })
                    .collect(),
                missing_index_codes: index_codes
                    .iter()
                    .filter(|ts_code| !existing_index_codes.contains(*ts_code))
                    .cloned()
                    .collect(),
                missing_index_ths_daily: !coverage.index_ths_daily_dates.contains(&trade_date),
                missing_index_moneyflow: !coverage.index_moneyflow_dates.contains(&trade_date),
            }
        })
        .filter(TargetCacheTradeDatePlan::has_work)
        .collect();

    TargetCachePlan {
        total_dates: dates.len(),
        work_items,
    }
}

fn merge_counts(target: &mut BTreeMap<String, usize>, source: &BTreeMap<String, usize>) {
    for (table, count) in source {
        *target.entry(table.clone()).or_insert(0) += count;
    }
}

pub async fn trade_cal_open_dates(
    config: &Config,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Result<Vec<NaiveDate>> {
    let mut command = PythonWorker::command_from_env();
    command.env("TUSHARE_TOKEN", &config.tushare_token);
    let mut worker = PythonWorker::spawn(command).await?;
    let params = worker_params(&[
        ("exchange", string_param("SSE")),
        (
            "start_date",
            string_param(start_date.format("%Y%m%d").to_string()),
        ),
        (
            "end_date",
            string_param(end_date.format("%Y%m%d").to_string()),
        ),
    ]);
    let rows = worker
        .fetch_api(
            "trade-cal",
            "trade_cal",
            "trade_cal",
            &["cal_date", "is_open"],
            &params,
        )
        .await?;
    open_dates_from_trade_cal_rows(&rows)
}

fn open_dates_from_trade_cal_rows(rows: &[SourceRow]) -> Result<Vec<NaiveDate>> {
    let mut dates = Vec::new();
    for row in rows {
        if !is_open_trade_cal_row(row) {
            continue;
        }
        let Some(cal_date) = row.get("cal_date").and_then(scalar_text) else {
            return Err(anyhow!("trade_cal returned open row without cal_date"));
        };
        dates.push(parse_date_text(&cal_date)?);
    }
    dates.sort_unstable();
    dates.dedup();
    Ok(dates)
}

fn is_open_trade_cal_row(row: &SourceRow) -> bool {
    row.get("is_open")
        .and_then(scalar_text)
        .is_some_and(|value| value == "1")
}

fn scalar_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(text.trim().to_string()),
        serde_json::Value::Number(number) => Some(number.to_string()),
        serde_json::Value::Bool(value) => Some(if *value { "1" } else { "0" }.to_string()),
        _ => None,
    }
}

fn enabled_index_codes(path: &str) -> Result<Vec<String>> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read INDEX_LIST_PATH {path}"))?;
    let mut codes = Vec::new();
    for (line_number, raw_line) in contents.lines().enumerate() {
        if line_number == 0 || raw_line.trim().is_empty() {
            continue;
        }
        let columns = raw_line.split(',').map(str::trim).collect::<Vec<_>>();
        if columns.len() < 4 {
            return Err(anyhow!(
                "invalid index list line {}: expected 4 columns",
                line_number + 1
            ));
        }
        if columns[3].eq_ignore_ascii_case("true") {
            codes.push(columns[0].to_string());
        }
    }
    Ok(codes)
}

async fn existing_trade_dates_in_window(
    client: &tokio_postgres::Client,
    table: &str,
    dates: &[NaiveDate],
) -> Result<HashSet<NaiveDate>> {
    let Some((start_date, end_date, target_dates)) = date_window_filter(dates) else {
        return Ok(HashSet::new());
    };
    let sql = format!(
        r#"select distinct "trade_date" from "{table}" where "trade_date" >= $1 and "trade_date" <= $2"#
    );
    let rows = client.query(&sql, &[&start_date, &end_date]).await?;
    Ok(rows
        .into_iter()
        .map(|row| row.get(0))
        .filter(|trade_date| target_dates.contains(trade_date))
        .collect())
}

async fn existing_ts_codes_by_date_in_window(
    client: &tokio_postgres::Client,
    table: &str,
    dates: &[NaiveDate],
) -> Result<BTreeMap<NaiveDate, HashSet<String>>> {
    let Some((start_date, end_date, target_dates)) = date_window_filter(dates) else {
        return Ok(BTreeMap::new());
    };
    let sql = format!(
        r#"select "trade_date", "ts_code" from "{table}" where "trade_date" >= $1 and "trade_date" <= $2"#
    );
    let rows = client.query(&sql, &[&start_date, &end_date]).await?;
    let mut codes_by_date = BTreeMap::new();
    for row in rows {
        let trade_date = row.get(0);
        if !target_dates.contains(&trade_date) {
            continue;
        }
        codes_by_date
            .entry(trade_date)
            .or_insert_with(HashSet::new)
            .insert(row.get(1));
    }
    Ok(codes_by_date)
}

fn date_window_filter(dates: &[NaiveDate]) -> Option<(NaiveDate, NaiveDate, HashSet<NaiveDate>)> {
    let start_date = dates.iter().copied().min()?;
    let end_date = dates.iter().copied().max()?;
    let target_dates = dates.iter().copied().collect();
    Some((start_date, end_date, target_dates))
}

fn trade_date_params(trade_date_text: &str) -> BTreeMap<String, serde_json::Value> {
    worker_params(&[("trade_date", string_param(trade_date_text))])
}

async fn fetch(
    worker: &mut PythonWorker,
    request_id: &str,
    dataset: &DatasetMetadata,
    params: &BTreeMap<String, serde_json::Value>,
) -> Result<Vec<SourceRow>> {
    worker.fetch_dataset(request_id, dataset, params).await
}

fn required_dataset(id: &str) -> Result<&'static DatasetMetadata> {
    dataset(id).ok_or_else(|| anyhow!("unknown generated dataset {id}"))
}

fn retain_rows_with_non_empty_field(rows: &mut Vec<SourceRow>, field: &str) -> usize {
    let original_len = rows.len();
    rows.retain(|row| match row.get(field) {
        Some(value) if !value.is_null() => {
            !matches!(value, serde_json::Value::String(text) if text.trim().is_empty())
        }
        _ => false,
    });
    original_len - rows.len()
}

fn resolve_cache_dates(window: &DateWindow) -> Result<Vec<NaiveDate>> {
    match window {
        DateWindow::Explicit {
            start_date,
            end_date,
        } => {
            let start = parse_date_text(start_date).context("invalid --start-date")?;
            let end = parse_date_text(end_date).context("invalid --end-date")?;
            let mut dates = Vec::new();
            let mut current = start;
            while current <= end {
                if current.weekday().number_from_monday() <= 5 {
                    dates.push(current);
                }
                current += ChronoDuration::days(1);
            }
            Ok(dates)
        }
        DateWindow::RefreshOnly => Ok(Vec::new()),
        DateWindow::LookbackTradingDays(_) | DateWindow::DefaultLookbackTradingDays(_) => {
            Err(anyhow!(
                "cache execution currently requires explicit --start-date/--end-date or --refresh-only"
            ))
        }
    }
}

pub fn summary_json(summary: &CacheSummary) -> Result<String> {
    Ok(serde_json::to_string_pretty(&json!(summary))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_cache_dates_skip_weekends() {
        let dates = resolve_cache_dates(&DateWindow::Explicit {
            start_date: "20260626".to_string(),
            end_date: "20260629".to_string(),
        })
        .unwrap();

        assert_eq!(
            dates
                .iter()
                .map(|date| date.format("%Y%m%d").to_string())
                .collect::<Vec<_>>(),
            vec!["20260626", "20260629"]
        );
    }

    #[test]
    fn default_lookback_is_not_silently_implemented() {
        assert!(resolve_cache_dates(&DateWindow::DefaultLookbackTradingDays(90)).is_err());
    }

    #[test]
    fn reads_enabled_index_codes() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            file.path(),
            "ts_code,name,category,enabled\n000001.SH,上证指数,market,true\n399001.SZ,深证成指,market,false\n399006.SZ,创业板指,market,TRUE\n",
        )
        .unwrap();

        assert_eq!(
            enabled_index_codes(file.path().to_str().unwrap()).unwrap(),
            vec!["000001.SH".to_string(), "399006.SZ".to_string()]
        );
    }

    #[test]
    fn parses_trade_cal_open_dates() {
        let rows = vec![
            BTreeMap::from([
                ("cal_date".to_string(), json!("20250101")),
                ("is_open".to_string(), json!(0)),
            ]),
            BTreeMap::from([
                ("cal_date".to_string(), json!("20250102")),
                ("is_open".to_string(), json!(1)),
            ]),
            BTreeMap::from([
                ("cal_date".to_string(), json!("20250103")),
                ("is_open".to_string(), json!("1")),
            ]),
        ];

        assert_eq!(
            open_dates_from_trade_cal_rows(&rows).unwrap(),
            vec![
                parse_date_text("20250102").unwrap(),
                parse_date_text("20250103").unwrap()
            ]
        );
    }

    #[test]
    fn trade_date_params_match_daily_python_function_shape() {
        let params = trade_date_params("20260625");

        assert_eq!(params.get("trade_date"), Some(&json!("20260625")));
        assert!(!params.contains_key("ts_code"));
        assert!(!params.contains_key("start_date"));
        assert!(!params.contains_key("end_date"));
    }

    #[test]
    fn target_plan_is_complete_when_all_daily_sources_have_coverage() {
        let dates = ["20250401", "20250402"]
            .into_iter()
            .map(|date| parse_date_text(date).unwrap())
            .collect::<Vec<_>>();
        let index_codes = vec!["000001.SH".to_string(), "399001.SZ".to_string()];
        let coverage = TargetExistingCoverage {
            daily_dataset_dates: DAILY_CACHE_DATASETS
                .iter()
                .map(|dataset_id| (*dataset_id, dates.iter().copied().collect()))
                .collect(),
            index_factor_codes_by_date: BTreeMap::from([
                (dates[0], index_codes.iter().cloned().collect()),
                (dates[1], index_codes.iter().cloned().collect()),
            ]),
            index_ths_daily_dates: dates.iter().copied().collect(),
            index_moneyflow_dates: dates.iter().copied().collect(),
        };

        let plan = target_cache_plan_from_coverage(&dates, &index_codes, &coverage);

        assert!(plan.is_complete());
        assert_eq!(plan.total_dates, 2);
    }

    #[test]
    fn target_plan_contains_only_missing_daily_source_work() {
        let dates = ["20250401", "20250402"]
            .into_iter()
            .map(|date| parse_date_text(date).unwrap())
            .collect::<Vec<_>>();
        let index_codes = vec!["000001.SH".to_string(), "399001.SZ".to_string()];
        let coverage = TargetExistingCoverage {
            daily_dataset_dates: BTreeMap::from([
                ("stock_stk_factor_pro", [dates[0]].into_iter().collect()),
                (
                    "stock_moneyflow_ths",
                    dates.iter().copied().collect::<HashSet<_>>(),
                ),
                ("stock_limit_list_ths", dates.iter().copied().collect()),
                ("index_limit_cpt_list", dates.iter().copied().collect()),
            ]),
            index_factor_codes_by_date: BTreeMap::from([
                (dates[0], index_codes.iter().cloned().collect()),
                (dates[1], ["000001.SH".to_string()].into_iter().collect()),
            ]),
            index_ths_daily_dates: [dates[0]].into_iter().collect(),
            index_moneyflow_dates: dates.iter().copied().collect(),
        };

        let plan = target_cache_plan_from_coverage(&dates, &index_codes, &coverage);

        assert_eq!(plan.total_dates, 2);
        assert_eq!(plan.daily_dataset_request_count(), 1);
        assert_eq!(plan.index_code_request_count(), 1);
        assert_eq!(plan.index_ths_daily_day_count(), 1);
        assert_eq!(plan.index_moneyflow_day_count(), 0);
        assert_eq!(
            plan.work_items,
            vec![TargetCacheTradeDatePlan {
                date_index: 1,
                trade_date: dates[1],
                missing_daily_datasets: vec!["stock_stk_factor_pro"],
                missing_index_codes: vec!["399001.SZ".to_string()],
                missing_index_ths_daily: true,
                missing_index_moneyflow: false,
            }]
        );
    }

    #[test]
    fn warmup_plan_is_complete_when_all_sources_have_coverage() {
        let dates = ["20260624", "20260625"]
            .into_iter()
            .map(|date| parse_date_text(date).unwrap())
            .collect::<Vec<_>>();
        let index_codes = vec!["000001.SH".to_string(), "399001.SZ".to_string()];
        let coverage = WarmupExistingCoverage {
            stock_factor_dates: dates.iter().copied().collect(),
            index_factor_dates: dates.iter().copied().collect(),
            index_ths_daily_dates: dates.iter().copied().collect(),
        };

        let plan = warmup_cache_plan_from_coverage(&dates, &index_codes, &coverage);

        assert!(plan.is_complete());
        assert_eq!(plan.total_dates, 2);
    }

    #[test]
    fn warmup_plan_contains_only_missing_source_work() {
        let dates = ["20260624", "20260625"]
            .into_iter()
            .map(|date| parse_date_text(date).unwrap())
            .collect::<Vec<_>>();
        let index_codes = vec!["000001.SH".to_string(), "399001.SZ".to_string()];
        let coverage = WarmupExistingCoverage {
            stock_factor_dates: [dates[0]].into_iter().collect(),
            index_factor_dates: [dates[0]].into_iter().collect(),
            index_ths_daily_dates: [dates[1]].into_iter().collect(),
        };

        let plan = warmup_cache_plan_from_coverage(&dates, &index_codes, &coverage);

        assert_eq!(plan.total_dates, 2);
        assert_eq!(plan.stock_factor_day_count(), 1);
        assert_eq!(plan.index_code_request_count(), 2);
        assert_eq!(plan.index_ths_daily_day_count(), 1);
        assert_eq!(
            plan.work_items,
            vec![
                WarmupCacheTradeDatePlan {
                    date_index: 0,
                    trade_date: dates[0],
                    missing_stock_factor: false,
                    missing_index_codes: Vec::new(),
                    missing_index_ths_daily: true,
                },
                WarmupCacheTradeDatePlan {
                    date_index: 1,
                    trade_date: dates[1],
                    missing_stock_factor: true,
                    missing_index_codes: vec!["000001.SH".to_string(), "399001.SZ".to_string()],
                    missing_index_ths_daily: false,
                },
            ]
        );
    }

    #[test]
    fn warmup_plan_does_not_backfill_new_index_codes_when_date_has_index_rows() {
        let date = parse_date_text("20230808").unwrap();
        let index_codes = vec!["000001.SH".to_string(), "000680.SH".to_string()];
        let coverage = WarmupExistingCoverage {
            stock_factor_dates: [date].into_iter().collect(),
            index_factor_dates: [date].into_iter().collect(),
            index_ths_daily_dates: [date].into_iter().collect(),
        };

        let plan = warmup_cache_plan_from_coverage(&[date], &index_codes, &coverage);

        assert!(plan.is_complete());
        assert_eq!(plan.index_code_request_count(), 0);
    }

    #[test]
    fn drops_rows_missing_required_source_code_before_moneyflow_write() {
        let mut rows = vec![
            BTreeMap::from([
                ("trade_date".to_string(), json!("20260520")),
                ("ts_code".to_string(), json!("885893.TI")),
                ("name".to_string(), json!("国家大基金持股")),
            ]),
            BTreeMap::from([
                ("trade_date".to_string(), json!("20260520")),
                ("ts_code".to_string(), serde_json::Value::Null),
                ("name".to_string(), json!("AI视频")),
            ]),
            BTreeMap::from([
                ("trade_date".to_string(), json!("20260520")),
                ("ts_code".to_string(), json!(" ")),
                ("name".to_string(), json!("empty")),
            ]),
            BTreeMap::from([
                ("trade_date".to_string(), json!("20260520")),
                ("name".to_string(), json!("missing")),
            ]),
        ];

        assert_eq!(retain_rows_with_non_empty_field(&mut rows, "ts_code"), 3);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("ts_code"), Some(&json!("885893.TI")));
    }
}
