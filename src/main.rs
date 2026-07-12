use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    stock_cache::run_stock_cache_cli().await
}
