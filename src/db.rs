use anyhow::Result;
use tokio_postgres::NoTls;

const SCHEMA_SQL: &str = include_str!("../db/schema.sql");

pub async fn init_db(postgres_dsn: &str) -> Result<()> {
    let (client, connection) = tokio_postgres::connect(postgres_dsn, NoTls).await?;
    let connection_task = tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("postgres connection error: {error}");
        }
    });

    let result = client.batch_execute(SCHEMA_SQL).await;
    connection_task.abort();
    result?;
    println!("{{\"status\":\"success\",\"command\":\"init-db\"}}");
    Ok(())
}
