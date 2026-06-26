mod cli;
mod config;
mod db;
mod generated;
mod write;
mod write_sql;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let env_file = config::load_env_file(cli.env_file.as_deref())?;
    let config = config::Config::from_env(env_file.as_ref())?;

    match cli.command {
        Commands::InitDb => db::init_db(&config.postgres_dsn).await,
        Commands::Config { command } => match command {
            cli::ConfigCommand::Show => {
                config.print_json()?;
                Ok(())
            }
        },
        Commands::Write(args) => write::run(args, &config).await,
    }
}
