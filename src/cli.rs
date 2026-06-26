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
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    Write(WriteArgs),
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    Show,
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
    pub refresh_ths: bool,
}
