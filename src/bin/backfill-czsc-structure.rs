use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    stock_cache::run_czsc_backfill_cli().await
}
