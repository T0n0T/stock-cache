use anyhow::Result;
use tokio_postgres::NoTls;

const SCHEMA_SQL: &str = include_str!("../../deploy/postgres/schema.sql");

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_schema_uses_non_destructive_idempotent_ddl() {
        assert!(SCHEMA_SQL.contains("create table if not exists"));
        assert!(SCHEMA_SQL.contains("create index if not exists"));
        assert!(!SCHEMA_SQL
            .lines()
            .any(|line| line.starts_with("create table \"")));
        assert!(!SCHEMA_SQL
            .lines()
            .any(|line| line.starts_with("create index \"")));
        assert!(!SCHEMA_SQL.contains("drop table"));
        assert!(!SCHEMA_SQL.contains("drop index"));
    }
}
