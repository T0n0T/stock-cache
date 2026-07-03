use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(name = "stock-cache")]
#[command(about = "PostgreSQL-backed Tushare stock cache CLI")]
pub struct Cli {
    #[arg(long, global = true)]
    pub env_file: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    InitDb,
    Status,
    Erase(EraseArgs),
    ContractCheck {
        #[command(subcommand)]
        command: ContractCheckCommand,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    Write(WriteArgs),
}

#[derive(Debug, Args)]
pub struct EraseArgs {
    #[arg(long)]
    pub start_date: Option<String>,

    #[arg(long)]
    pub end_date: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    Show,
}

#[derive(Debug, Subcommand)]
pub enum ContractCheckCommand {
    StockSelect(StockSelectContractArgs),
}

#[derive(Debug, Args)]
pub struct StockSelectContractArgs {
    #[arg(long)]
    pub pick_date: Option<String>,

    #[arg(long, default_value_t = 252)]
    pub lookback_trading_days: i64,

    #[arg(long, default_value_t = 0.995)]
    pub period_coverage_threshold: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum WriteMode {
    Full,
    Cache,
    Calc,
}

#[derive(Debug, Args)]
pub struct WriteArgs {
    #[arg(long, value_enum)]
    pub mode: WriteMode,

    #[arg(long)]
    pub start_date: Option<String>,

    #[arg(long)]
    pub end_date: Option<String>,

    #[arg(long)]
    pub lookback_trading_days: Option<u32>,

    #[arg(long)]
    pub max_concurrency: Option<u32>,

    #[arg(long)]
    pub chunk_size: Option<usize>,

    #[arg(long, alias = "refresh-ths")]
    pub refresh: bool,

    #[arg(long)]
    pub force_recalc: bool,
}
