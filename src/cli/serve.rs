//! Command-line arguments for the Quack server.

use std::path::PathBuf;

use anyhow::Result;
use clap::Args as ClapArgs;

#[derive(Debug, ClapArgs)]
pub(crate) struct Args {
    /// Existing DuckDB database (DUCKDB_PATH; default: runtime/stock-cache.duckdb).
    #[arg(long, value_name = "PATH")]
    pub(crate) database: Option<PathBuf>,
    /// Quack listen address (QUACK_BIND; default: 127.0.0.1:9494).
    #[arg(long, value_name = "HOST:PORT")]
    pub(crate) bind: Option<String>,
    /// Required nonempty Quack token (QUACK_AUTH_TOKEN).
    #[arg(long, value_name = "TOKEN")]
    pub(crate) auth_token: Option<String>,
}

pub(crate) fn run(args: Args) -> Result<()> {
    crate::app::serve::run(crate::app::serve::Options {
        database: args.database,
        bind: args
            .bind
            .or_else(|| std::env::var("QUACK_BIND").ok())
            .unwrap_or_else(|| "127.0.0.1:9494".to_owned()),
        auth_token: args
            .auth_token
            .or_else(|| std::env::var("QUACK_AUTH_TOKEN").ok()),
    })
}
