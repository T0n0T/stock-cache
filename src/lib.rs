mod app;
mod cli;
mod db;
mod metadata;
mod source;

mod calc;

use anyhow::Result;
use clap::Parser;

use app::{config, contract_check, erase, status, write};
use cli::{Cli, Commands, ContractCheckCommand};

pub use calc::backfill::run_czsc_backfill_cli;

pub async fn run_stock_cache_cli() -> Result<()> {
    let cli = Cli::parse();
    let env_file = config::load_env_file(cli.env_file.as_deref())?;
    let config = config::Config::from_env_with_options(
        env_file.as_ref(),
        command_requires_tushare_token(&cli.command),
    )?;

    match cli.command {
        Commands::InitDb => db::init_db(&config.postgres_dsn).await,
        Commands::Status => status::run(&config).await,
        Commands::Erase(args) => erase::run(args, &config).await,
        Commands::ContractCheck { command } => match command {
            ContractCheckCommand::StockSelect(args) => {
                contract_check::run_stock_select(args, &config).await
            }
        },
        Commands::Config { command } => match command {
            cli::ConfigCommand::Show => {
                config.print_human_readable();
                Ok(())
            }
        },
        Commands::Write(args) => write::run(args, &config).await,
    }
}

fn command_requires_tushare_token(command: &Commands) -> bool {
    matches!(command, Commands::Config { .. })
        || matches!(command, Commands::Write(args) if args.mode != cli::WriteMode::Calc)
}
