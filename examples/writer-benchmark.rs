//! Developer-only harness for measuring the batch writer without adding product commands.
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::{collections::BTreeMap, path::PathBuf};
use stock_cache::v3::duckdb;

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Init {
        #[arg(long)]
        database: String,
    },
    Import {
        #[arg(long)]
        database: String,
        #[arg(long)]
        table: String,
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        strategy: String,
        #[arg(long)]
        dates: Option<String>,
    },
}
fn main() -> Result<()> {
    match Args::parse().command {
        Command::Init { database } => duckdb::with_connection(&database, |connection| {
            duckdb::create_schema(connection)?;
            duckdb::checkpoint(connection)?;
            println!("{{\"status\":\"ok\"}}");
            Ok(())
        }),
        Command::Import {
            database,
            table,
            input,
            strategy,
            dates,
        } => duckdb::with_connection(&database, |connection| {
            let dates = dates
                .map(|s| s.split(',').map(str::to_string).collect::<Vec<_>>())
                .unwrap_or_default();
            let written = duckdb::write_jsonl(
                connection,
                &table,
                &input,
                &strategy,
                &dates,
                &BTreeMap::new(),
                &[],
            )?;
            duckdb::checkpoint(connection)?;
            println!("{}", serde_json::json!({"written_rows":written}));
            Ok(())
        }),
    }
}
