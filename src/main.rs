//! Rust-owned DuckDB command-line interface.

mod cli;

use std::{collections::BTreeMap, fs, io::Read, path::PathBuf, time::Instant};

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use serde_json::Value;
use stock_cache::metadata::tushare_fields::{dataset, WriteStrategy};
use stock_cache::v3::duckdb;
use stock_cache::{TushareHttpClient, TushareHttpConfig};

#[derive(Debug, Parser)]
#[command(name = "stock-cache")]
struct Cli {
    #[arg(long, global = true)]
    env_file: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Fetch market data, prepare historical inputs, calculate and persist.
    Write(cli::write::Args),
    /// Compute indicators and optionally persist calculation tables.
    Calc(cli::calc::Args),
    #[command(alias = "init-db")]
    Init {
        #[arg(long)]
        database: Option<String>,
    },
    Status {
        #[arg(long)]
        database: Option<String>,
    },
    Fetch {
        #[arg(long)]
        database: String,
        /// Generated Tushare dataset id, for example stock_stk_factor_pro.
        #[arg(long)]
        dataset: String,
        /// JSON object passed as Tushare params.
        #[arg(long, default_value = "{}")]
        params: String,
    },
    Import {
        #[arg(long)]
        database: String,
        #[arg(long)]
        table: String,
        #[arg(long, default_value = "-")]
        input: PathBuf,
        #[arg(long, default_value = "append", value_parser = ["append", "replace-dates", "replace-keys"])]
        strategy: String,
        #[arg(long)]
        dates: Option<String>,
        #[arg(long = "scope")]
        scope: Vec<String>,
        #[arg(long = "key-values")]
        key_values: Option<String>,
    },
}

fn fetch_dataset(database: &str, dataset_id: &str, raw_params: &str) -> Result<()> {
    let metadata = dataset(dataset_id)
        .ok_or_else(|| anyhow::anyhow!("unknown generated Tushare dataset: {dataset_id}"))?;
    let params = serde_json::from_str::<BTreeMap<String, Value>>(raw_params)
        .map_err(|error| anyhow::anyhow!("--params must be a JSON object: {error}"))?;
    let total_started = Instant::now();
    let http_started = Instant::now();
    let runtime = tokio::runtime::Runtime::new()?;
    let rows = runtime.block_on(async {
        let client = TushareHttpClient::new(TushareHttpConfig::from_env()?)?;
        client.fetch_dataset(metadata, &params).await
    })?;
    let http_ms = http_started.elapsed().as_secs_f64() * 1000.0;
    let temp = std::env::temp_dir().join(format!(
        "stock-cache-fetch-{}-{}.jsonl",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or_default()
    ));
    let serialize_started = Instant::now();
    {
        let mut file = std::fs::File::create(&temp)?;
        for row in &rows {
            serde_json::to_writer(&mut file, row)?;
            std::io::Write::write_all(&mut file, b"\n")?;
        }
    }
    let serialize_ms = serialize_started.elapsed().as_secs_f64() * 1000.0;
    let input_bytes = std::fs::metadata(&temp)?.len();
    let dates = rows
        .iter()
        .filter_map(|row| row.get("trade_date").and_then(Value::as_str))
        .map(str::to_string)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut key_values = Vec::new();
    let strategy = match metadata.write_strategy {
        WriteStrategy::DeleteInsertByDate { .. } | WriteStrategy::Upsert { .. }
            if !dates.is_empty() =>
        {
            "replace-dates"
        }
        WriteStrategy::ReplaceAll { key, .. } if dates.is_empty() => {
            key_values = rows
                .iter()
                .map(|row| {
                    key.iter()
                        .map(|column| row.get(*column).cloned().unwrap_or(Value::Null))
                        .collect()
                })
                .collect();
            "replace-keys"
        }
        _ if !dates.is_empty() => "replace-dates",
        _ => "append",
    };
    let db_started = Instant::now();
    let result = duckdb::with_connection(database, |connection| {
        duckdb::create_schema(connection)?;
        let written = duckdb::write_jsonl(
            connection,
            metadata.table,
            &temp,
            strategy,
            &dates,
            &BTreeMap::new(),
            &key_values,
        )?;
        duckdb::checkpoint(connection)?;
        let db_write_ms = db_started.elapsed().as_secs_f64() * 1000.0;
        let total_ms = total_started.elapsed().as_secs_f64() * 1000.0;
        let rows_per_sec = if total_ms > 0.0 {
            rows.len() as f64 / (total_ms / 1000.0)
        } else {
            0.0
        };
        let db_rows_per_sec = if db_write_ms > 0.0 {
            written as f64 / (db_write_ms / 1000.0)
        } else {
            0.0
        };
        let input_mib = input_bytes as f64 / (1024.0 * 1024.0);
        let input_mib_per_sec = if total_ms > 0.0 {
            input_mib / (total_ms / 1000.0)
        } else {
            0.0
        };
        println!(
            "{}",
            serde_json::json!({"status":"ok", "dataset":dataset_id, "table":metadata.table, "strategy":strategy, "fetched_rows":rows.len(), "written_rows":written, "http_ms":http_ms, "serialize_ms":serialize_ms, "db_write_ms":db_write_ms, "total_ms":total_ms, "rows_per_sec":rows_per_sec, "db_rows_per_sec":db_rows_per_sec, "input_bytes":input_bytes, "input_mib":input_mib, "input_mib_per_sec":input_mib_per_sec})
        );
        Ok(())
    });
    let _ = std::fs::remove_file(temp);
    result
}

fn parse_scope(values: &[String]) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    for value in values {
        let Some((key, raw)) = value.split_once('=') else {
            bail!("scope must be key=value: {value}")
        };
        result.insert(key.to_string(), raw.to_string());
    }
    Ok(result)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    cli::write::load_env(cli.env_file.as_deref())?;
    match cli.command {
        Command::Write(args) => cli::write::run(args),
        Command::Calc(args) => cli::calc::run(args),
        Command::Init { database } => {
            let database = resolve_database(database)?;
            duckdb::with_connection(&database, |connection| {
                duckdb::create_schema(connection)?;
                duckdb::checkpoint(connection)?;
                println!(
                    "{{\"status\":\"ok\",\"database\":{}}}",
                    serde_json::to_string(&database)?
                );
                Ok(())
            })
        }
        Command::Status { database } => {
            let database = resolve_database(database)?;
            duckdb::with_connection(&database, |connection| {
                duckdb::create_schema(connection)?;
                let tables = [
                    "cache_trade_calendar",
                    "cache_fetch_coverage",
                    "stock_basic",
                    "stock_stk_factor_pro",
                    "stock_moneyflow_ths",
                    "index_idx_factor_pro",
                    "index_ths",
                    "index_ths_daily",
                    "index_moneyflow_cnt_ths",
                    "index_ths_member",
                    "index_limit_cpt_list",
                    "stock_limit_list_ths",
                    "source_daily_bars",
                    "calc_period_bars",
                    "calc_indicator_state",
                    "calc_daily_indicators",
                    "calc_daily_czsc_structure",
                    "calc_daily_rolling_factors",
                    "calc_daily_left_peak",
                ];
                let mut counts = serde_json::Map::new();
                for table in tables {
                    counts.insert(
                        table.to_string(),
                        Value::from(duckdb::table_count(connection, table)?),
                    );
                }
                println!(
                    "{}",
                    serde_json::json!({"database": database, "tables": counts})
                );
                Ok(())
            })
        }
        Command::Fetch {
            database,
            dataset: dataset_id,
            params,
        } => fetch_dataset(&database, &dataset_id, &params),
        Command::Import {
            database,
            table,
            input,
            strategy,
            dates,
            scope,
            key_values,
        } => {
            let stdin_path = if input.as_os_str() == "-" {
                let path = std::env::temp_dir().join(format!(
                    "stock-cache-stdin-{}-{}.jsonl",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|value| value.as_nanos())
                        .unwrap_or_default()
                ));
                let mut bytes = Vec::new();
                std::io::stdin().read_to_end(&mut bytes)?;
                fs::write(&path, bytes)?;
                Some(path)
            } else {
                None
            };
            let input = stdin_path.as_deref().unwrap_or(&input);
            let dates: Vec<String> = dates
                .map(|value| value.split(',').map(str::to_string).collect::<Vec<_>>())
                .unwrap_or_default();
            let keys = key_values
                .map(|value| serde_json::from_str::<Vec<Vec<Value>>>(&value))
                .transpose()?
                .unwrap_or_default();
            let scope = parse_scope(&scope)?;
            let result = duckdb::with_connection(&database, |connection| {
                duckdb::create_schema(connection)?;
                let written = duckdb::write_jsonl(
                    connection, &table, input, &strategy, &dates, &scope, &keys,
                )?;
                duckdb::checkpoint(connection)?;
                println!(
                    "{}",
                    serde_json::json!({"status":"ok", "table":table, "strategy":strategy, "written_rows":written})
                );
                Ok(())
            });
            if let Some(path) = stdin_path {
                let _ = fs::remove_file(path);
            }
            result
        }
    }
}

fn resolve_database(database: Option<String>) -> Result<String> {
    let database = database
        .or_else(|| std::env::var("DUCKDB_PATH").ok())
        .unwrap_or_else(|| "runtime/stock-cache.duckdb".into());
    if let Some(parent) = std::path::Path::new(&database)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    Ok(database)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn market_write_accepts_v2_entrypoint_and_rejects_ambiguous_windows() {
        assert!(matches!(
            Cli::try_parse_from([
                "stock-cache",
                "write",
                "--mode",
                "full",
                "--lookback-trading-days",
                "1"
            ])
            .unwrap()
            .command,
            Command::Write(_)
        ));
        assert!(Cli::try_parse_from(["stock-cache", "init-db"]).is_ok());
        assert!(Cli::try_parse_from(["stock-cache", "status"]).is_ok());
        for args in [
            vec![
                "stock-cache",
                "write",
                "--mode",
                "full",
                "--lookback-trading-days",
                "0",
            ],
            vec![
                "stock-cache",
                "write",
                "--mode",
                "full",
                "--start-date",
                "2026-11-09",
            ],
            vec![
                "stock-cache",
                "write",
                "--mode",
                "full",
                "--lookback-trading-days",
                "1",
                "--start-date",
                "2026-11-09",
                "--end-date",
                "2026-11-13",
            ],
        ] {
            assert!(Cli::try_parse_from(args).is_err());
        }
    }
}
