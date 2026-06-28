mod app;
mod calc;
mod cli;
mod db;
mod metadata;
mod source;

use anyhow::Result;
use app::{config, status, write};
use clap::Parser;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let env_file = config::load_env_file(cli.env_file.as_deref())?;
    let config = config::Config::from_env_with_options(
        env_file.as_ref(),
        command_requires_tushare_token(&cli.command),
    )?;

    match cli.command {
        Commands::InitDb => db::init_db(&config.postgres_dsn).await,
        Commands::Status => status::run(&config).await,
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
