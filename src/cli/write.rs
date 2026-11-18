//! Command-line arguments for the market-cache write workflow.
use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, ensure, Result};
use clap::{Args as ClapArgs, ValueEnum};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Mode {
    Full,
    Cache,
    Calc,
}

#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    #[arg(long, value_enum)]
    mode: Mode,
    #[arg(long)]
    database: Option<PathBuf>,
    #[arg(long, requires = "end_date", conflicts_with = "lookback_trading_days")]
    start_date: Option<String>,
    #[arg(
        long,
        requires = "start_date",
        conflicts_with = "lookback_trading_days"
    )]
    end_date: Option<String>,
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    lookback_trading_days: Option<u32>,
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    max_concurrency: Option<u32>,
    /// Number of instruments calculated per bounded memory batch.
    #[arg(long)]
    chunk_size: Option<usize>,
    #[arg(long, alias = "refresh-ths")]
    refresh: bool,
    /// Re-fetch the required calculation history before recalculating.
    #[arg(long)]
    force_recalc: bool,
    /// Skip requests already recorded as covered, including target dates.
    /// Use this to resume an interrupted backfill without refetching completed dates.
    #[arg(long, alias = "resume", conflicts_with = "force_recalc")]
    skip_covered: bool,
}

pub(crate) fn run(args: Args) -> Result<()> {
    let mode = match args.mode {
        Mode::Full => crate::app::write::Mode::Full,
        Mode::Cache => crate::app::write::Mode::Cache,
        Mode::Calc => crate::app::write::Mode::Calc,
    };
    crate::app::write::run(crate::app::write::Options {
        mode,
        database: args.database,
        start_date: args.start_date,
        end_date: args.end_date,
        lookback_trading_days: args.lookback_trading_days,
        max_concurrency: args.max_concurrency,
        chunk_size: args.chunk_size,
        refresh: args.refresh,
        force_recalc: args.force_recalc,
        skip_covered: args.skip_covered,
    })
}

pub(crate) fn load_env(path: Option<&Path>) -> Result<()> {
    let default = Path::new(".env");
    let Some(path) = path.or_else(|| default.is_file().then_some(default)) else {
        return Ok(());
    };
    for (index, line) in fs::read_to_string(path)?.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .strip_prefix("export ")
            .unwrap_or(line)
            .split_once('=')
            .ok_or_else(|| anyhow!("invalid env file line {}", index + 1))?;
        let key = key.trim();
        ensure!(
            !key.is_empty() && key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_'),
            "invalid env key at line {}",
            index + 1
        );
        if std::env::var_os(key).is_none() {
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                .unwrap_or(value);
            std::env::set_var(key, value);
        }
    }
    Ok(())
}
