//! Command-line arguments for date-window erasure.
use anyhow::Result;
use clap::Args as ClapArgs;

#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    #[arg(long)]
    pub(crate) database: Option<String>,
    /// Inclusive window start (YYYY-MM-DD); requires --end-date.
    #[arg(
        long,
        requires = "end_date",
        required_unless_present = "lookback_trading_days",
        conflicts_with = "lookback_trading_days"
    )]
    pub(crate) start_date: Option<String>,
    /// Inclusive window end (YYYY-MM-DD); requires --start-date.
    #[arg(
        long,
        requires = "start_date",
        required_unless_present = "lookback_trading_days",
        conflicts_with = "lookback_trading_days"
    )]
    pub(crate) end_date: Option<String>,
    /// Erase the most recent N cached trading sessions instead of an explicit window.
    #[arg(
        long,
        value_parser = clap::value_parser!(u32).range(1..),
        conflicts_with_all = ["start_date", "end_date"]
    )]
    pub(crate) lookback_trading_days: Option<u32>,
}

pub(crate) fn run(args: Args) -> Result<()> {
    crate::app::erase::run(crate::app::erase::Args {
        database: args.database,
        start_date: args.start_date,
        end_date: args.end_date,
        lookback_trading_days: args.lookback_trading_days,
    })
}
