//! Generic DuckDB table inspection used by application status and benchmarks.

use anyhow::{anyhow, Result};
use duckdb_neo::{connection::Connection, Parameters};

use super::schema::ident;

pub fn table_count(connection: &Connection, table: &str) -> Result<i64> {
    let mut result = connection.query(
        format!("SELECT COUNT(*)::BIGINT FROM {}", ident(table)),
        Parameters::None,
    )?;
    let chunk = result
        .next_chunk()?
        .ok_or_else(|| anyhow!("DuckDB returned no count row"))?;
    Ok(*chunk
        .get_vector_at::<i64>(0)?
        .reader()?
        .get(0)?
        .ok_or_else(|| anyhow!("DuckDB count is NULL"))?)
}
