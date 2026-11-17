//! Command-line arguments for date-window erasure.
use anyhow::Result;
use clap::Args as ClapArgs;

#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    #[arg(long)]
    pub(crate) database: Option<String>,
    #[arg(long, requires = "end_date", required = true)]
    pub(crate) start_date: String,
    #[arg(long, requires = "start_date", required = true)]
    pub(crate) end_date: String,
}

pub(crate) fn run(args: Args) -> Result<()> {
    crate::app::erase::run(crate::app::erase::Args {
        database: args.database,
        start_date: args.start_date,
        end_date: args.end_date,
    })
}
