//! Persisted calculation checkpoints.

use anyhow::{anyhow, Result};
use duckdb_neo::{connection::Connection, Parameters};

use crate::calc::checkpoint::IndicatorCheckpoint;

use super::schema::{parse_date, sql_string};

/// Load all persisted indicator states for one source instrument and calculation version.
///
/// Dates are read as text to keep the DuckDB C API boundary independent of its
/// temporal representation, then parsed with the same date rules as the writer.
pub fn load_indicator_checkpoints(
    connection: &Connection,
    source_dataset: &str,
    ts_code: &str,
    calc_version: &str,
) -> Result<Vec<IndicatorCheckpoint>> {
    let sql = format!(
        "SELECT source_dataset, ts_code, period_type, period_start_date::VARCHAR, period_end_date::VARCHAR, macd_ema12, macd_ema26, macd_dea, macd_period_count, kdj_k, kdj_d, kdj_period_count, calc_version FROM calc_indicator_state WHERE source_dataset = {} AND ts_code = {} AND calc_version = {} ORDER BY period_end_date, period_type",
        sql_string(source_dataset),
        sql_string(ts_code),
        sql_string(calc_version),
    );
    let mut result = connection.query(sql, Parameters::None)?;
    let mut rows = Vec::new();
    while let Some(chunk) = result.next_chunk()? {
        let source = chunk.get_vector_at::<String>(0)?.reader()?;
        let code = chunk.get_vector_at::<String>(1)?.reader()?;
        let period = chunk.get_vector_at::<String>(2)?.reader()?;
        let start = chunk.get_vector_at::<String>(3)?.reader()?;
        let end = chunk.get_vector_at::<String>(4)?.reader()?;
        let ema12 = chunk.get_vector_at::<f64>(5)?.reader()?;
        let ema26 = chunk.get_vector_at::<f64>(6)?.reader()?;
        let dea = chunk.get_vector_at::<f64>(7)?.reader()?;
        let macd_count = chunk.get_vector_at::<i64>(8)?.reader()?;
        let kdj_k = chunk.get_vector_at::<f64>(9)?.reader()?;
        let kdj_d = chunk.get_vector_at::<f64>(10)?.reader()?;
        let kdj_count = chunk.get_vector_at::<i64>(11)?.reader()?;
        let version = chunk.get_vector_at::<String>(12)?.reader()?;
        for index in 0..chunk.row_count()? {
            let source = source
                .get(index)?
                .ok_or_else(|| anyhow!("checkpoint source_dataset is NULL"))?;
            let code = code
                .get(index)?
                .ok_or_else(|| anyhow!("checkpoint ts_code is NULL"))?;
            let period = period
                .get(index)?
                .ok_or_else(|| anyhow!("checkpoint period_type is NULL"))?;
            let start = start
                .get(index)?
                .ok_or_else(|| anyhow!("checkpoint period_start_date is NULL"))?;
            let end = end
                .get(index)?
                .ok_or_else(|| anyhow!("checkpoint period_end_date is NULL"))?;
            let version = version
                .get(index)?
                .ok_or_else(|| anyhow!("checkpoint calc_version is NULL"))?;
            rows.push(IndicatorCheckpoint {
                source_dataset: source.to_string(),
                ts_code: code.to_string(),
                period_type: period.to_string(),
                period_start_date: parse_date(start)?,
                period_end_date: parse_date(end)?,
                macd_ema12: ema12.get(index)?.copied(),
                macd_ema26: ema26.get(index)?.copied(),
                macd_dea: dea.get(index)?.copied(),
                macd_period_count: macd_count.get(index)?.copied(),
                kdj_k: kdj_k.get(index)?.copied(),
                kdj_d: kdj_d.get(index)?.copied(),
                kdj_period_count: kdj_count.get(index)?.copied(),
                calc_version: version.to_string(),
            });
        }
    }
    Ok(rows)
}
