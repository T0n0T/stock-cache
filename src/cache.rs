use std::{
    collections::{BTreeMap, HashSet},
    fs,
    time::{Duration as StdDuration, Instant},
};

use anyhow::{anyhow, Context, Result};
use chrono::{Datelike, Duration as ChronoDuration, NaiveDate};
use serde::Serialize;
use serde_json::json;
use tokio::time::sleep;
use tokio_postgres::NoTls;

use crate::{
    config::Config,
    db::source_writer::{parse_date_text, replace_classification_group, write_dataset, SourceRow},
    generated::tushare_fields::{dataset, DatasetMetadata},
    tushare::python_worker::{string_param, worker_params, PythonWorker},
    write::DateWindow,
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
    pub refresh_ths: bool,
    pub source_row_counts: BTreeMap<String, usize>,
    pub message: &'static str,
}

pub async fn run_cache(
    window: &DateWindow,
    refresh_ths: bool,
    config: &Config,
) -> Result<CacheSummary> {
    let dates = resolve_cache_dates(window)?;
    run_cache_for_trade_dates(&dates, refresh_ths, config).await
}

pub async fn run_cache_for_trade_dates(
    dates: &[NaiveDate],
    refresh_ths: bool,
    config: &Config,
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
    let total_dates = dates.len();

    if refresh_ths {
        let started = Instant::now();
        eprintln!("[cache] refresh-ths start");
        let index_ths = required_dataset("index_ths")?;
        let index_ths_member = required_dataset("index_ths_member")?;
        let index_ths_rows = fetch(
            &mut worker,
            "refresh-ths-index",
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
                    "[cache] refresh-ths member {}/{} {}",
                    member_index + 1,
                    total_index_ths_rows,
                    ts_code
                );
            }
            let params = worker_params(&[("ts_code", string_param(ts_code))]);
            let mut rows = fetch(
                &mut worker,
                &format!("refresh-ths-member-{ts_code}"),
                index_ths_member,
                &params,
            )
            .await?;
            index_ths_member_rows.append(&mut rows);
            sleep(StdDuration::from_millis(THS_MEMBER_REQUEST_DELAY_MS)).await;
        }
        replace_classification_group(
            &mut client,
            index_ths,
            &index_ths_rows,
            index_ths_member,
            &index_ths_member_rows,
        )
        .await?;
        row_counts.insert(index_ths.id.to_string(), index_ths_rows.len());
        row_counts.insert(index_ths_member.id.to_string(), index_ths_member_rows.len());
        eprintln!(
            "[cache] refresh-ths done index_ths={} index_ths_member={} elapsed={:.1}s",
            index_ths_rows.len(),
            index_ths_member_rows.len(),
            started.elapsed().as_secs_f64()
        );
    }

    let index_codes = if dates.is_empty() {
        Vec::new()
    } else {
        enabled_index_codes(&config.index_list_path)?
    };
    for (date_index, trade_date) in dates.iter().enumerate() {
        let day_started = Instant::now();
        let trade_date_text = trade_date.format("%Y%m%d").to_string();
        eprintln!(
            "[cache] day {}/{} {} start",
            date_index + 1,
            total_dates,
            trade_date_text
        );
        for dataset_id in DAILY_CACHE_DATASETS {
            let dataset = required_dataset(dataset_id)?;
            if date_has_rows(&client, dataset.id, *trade_date).await? {
                eprintln!("[cache] {dataset_id} {trade_date_text} skip existing");
                continue;
            }
            let params = worker_params(&[("trade_date", string_param(&trade_date_text))]);
            let rows = fetch(
                &mut worker,
                &format!("{dataset_id}-{trade_date_text}"),
                dataset,
                &params,
            )
            .await?;
            let delete_date = if *dataset_id == "index_limit_cpt_list" {
                Some(*trade_date)
            } else {
                None
            };
            write_dataset(&mut client, dataset, &rows, delete_date).await?;
            *row_counts.entry((*dataset_id).to_string()).or_insert(0) += rows.len();
        }

        let index_dataset = required_dataset("index_idx_factor_pro")?;
        let existing_index_codes =
            existing_ts_codes_for_date(&client, index_dataset.id, *trade_date).await?;
        let mut missing_index_codes = 0usize;
        for ts_code in &index_codes {
            if existing_index_codes.contains(ts_code) {
                continue;
            }
            missing_index_codes += 1;
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
        if missing_index_codes == 0 {
            eprintln!("[cache] index_idx_factor_pro {trade_date_text} skip existing codes");
        }

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

        let moneyflow_cnt = required_dataset("index_moneyflow_cnt_ths")?;
        if date_has_rows(&client, moneyflow_cnt.id, *trade_date).await? {
            eprintln!("[cache] index_moneyflow_cnt_ths {trade_date_text} skip existing");
            eprintln!(
                "[cache] day {}/{} {} done elapsed={:.1}s",
                date_index + 1,
                total_dates,
                trade_date_text,
                day_started.elapsed().as_secs_f64()
            );
            continue;
        }
        let params = trade_date_params(&trade_date_text);
        let rows = fetch(
            &mut worker,
            &format!("index_moneyflow_cnt_ths-{trade_date_text}"),
            moneyflow_cnt,
            &params,
        )
        .await?;
        write_dataset(&mut client, moneyflow_cnt, &rows, None).await?;
        *row_counts.entry(moneyflow_cnt.id.to_string()).or_insert(0) += rows.len();
        eprintln!(
            "[cache] day {}/{} {} done elapsed={:.1}s",
            date_index + 1,
            total_dates,
            trade_date_text,
            day_started.elapsed().as_secs_f64()
        );
    }

    connection_task.abort();
    Ok(CacheSummary {
        status: "success",
        mode: "cache",
        refresh_ths,
        source_row_counts: row_counts,
        message: "source cache write completed",
    })
}

pub async fn run_calc_source_cache_for_trade_dates(
    dates: &[NaiveDate],
    config: &Config,
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
    let index_ths_daily = required_dataset("index_ths_daily")?;
    let total_dates = dates.len();

    for (date_index, trade_date) in dates.iter().enumerate() {
        let day_started = Instant::now();
        let trade_date_text = trade_date.format("%Y%m%d").to_string();
        if date_has_rows(&client, stock_dataset.id, *trade_date).await? {
            eprintln!(
                "[warmup] stock_stk_factor_pro day {}/{} {} skip existing",
                date_index + 1,
                total_dates,
                trade_date_text
            );
        } else {
            eprintln!(
                "[warmup] stock_stk_factor_pro day {}/{} {} start",
                date_index + 1,
                total_dates,
                trade_date_text
            );
            let params = worker_params(&[("trade_date", string_param(&trade_date_text))]);
            let rows = fetch(
                &mut worker,
                &format!("stock_stk_factor_pro-warmup-{trade_date_text}"),
                stock_dataset,
                &params,
            )
            .await?;
            write_dataset(&mut client, stock_dataset, &rows, None).await?;
            *row_counts.entry(stock_dataset.id.to_string()).or_insert(0) += rows.len();
            eprintln!(
                "[warmup] stock_stk_factor_pro day {}/{} {} done rows={} elapsed={:.1}s",
                date_index + 1,
                total_dates,
                trade_date_text,
                rows.len(),
                day_started.elapsed().as_secs_f64()
            );
        }

        let params = trade_date_params(&trade_date_text);
        let rows = fetch(
            &mut worker,
            &format!("index_ths_daily-warmup-{trade_date_text}"),
            index_ths_daily,
            &params,
        )
        .await?;
        write_dataset(&mut client, index_ths_daily, &rows, None).await?;
        *row_counts
            .entry(index_ths_daily.id.to_string())
            .or_insert(0) += rows.len();
    }

    connection_task.abort();
    Ok(CacheSummary {
        status: "success",
        mode: "cache",
        refresh_ths: false,
        source_row_counts: row_counts,
        message: "calc source cache warmup completed",
    })
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

async fn date_has_rows(
    client: &tokio_postgres::Client,
    table: &str,
    trade_date: NaiveDate,
) -> Result<bool> {
    let sql = format!(r#"select exists(select 1 from "{table}" where "trade_date" = $1)"#);
    let row = client.query_one(&sql, &[&trade_date]).await?;
    Ok(row.get(0))
}

async fn existing_ts_codes_for_date(
    client: &tokio_postgres::Client,
    table: &str,
    trade_date: NaiveDate,
) -> Result<HashSet<String>> {
    let sql = format!(r#"select "ts_code" from "{table}" where "trade_date" = $1"#);
    let rows = client.query(&sql, &[&trade_date]).await?;
    Ok(rows.into_iter().map(|row| row.get(0)).collect())
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

fn resolve_cache_dates(window: &DateWindow) -> Result<Vec<NaiveDate>> {
    match window {
        DateWindow::Explicit { start_date, end_date } => {
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
        DateWindow::RefreshThsOnly => Ok(Vec::new()),
        DateWindow::LookbackTradingDays(_) | DateWindow::DefaultLookbackTradingDays(_) => Err(anyhow!(
            "cache execution currently requires explicit --start-date/--end-date or --refresh-ths-only"
        )),
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
}
