//! Rust-owned DuckDB command-line interface.

mod cli;

use std::{fs, path::PathBuf};

use anyhow::Result;
use clap::{Parser, Subcommand};
use serde_json::Value;
use stock_cache::v3::duckdb;

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
    /// Erase source and calculation data in an inclusive date window.
    Erase(cli::erase::Args),
    #[command(alias = "init-db")]
    Init {
        #[arg(long)]
        database: Option<String>,
    },
    Status {
        #[arg(long)]
        database: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    cli::write::load_env(cli.env_file.as_deref())?;
    match cli.command {
        Command::Write(args) => cli::write::run(args),
        Command::Erase(args) => cli::erase::run(args),
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
        for removed in ["import", "fetch", "calc"] {
            assert!(Cli::try_parse_from(["stock-cache", removed]).is_err());
        }
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
