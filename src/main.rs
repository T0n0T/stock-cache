//! Rust-owned DuckDB command-line interface.

mod app;
mod cli;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

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
    /// Serve the existing DuckDB cache read-only over Quack until interrupted.
    Serve(cli::serve::Args),
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
        Command::Serve(args) => cli::serve::run(args),
        Command::Init { database } => app::database::init(database),
        Command::Status { database } => app::database::status(database),
    }
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
        assert!(Cli::try_parse_from(["stock-cache", "serve"]).is_ok());
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
