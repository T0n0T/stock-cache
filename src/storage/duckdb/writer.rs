//! Set-based DuckDB writer for source and calculated batches.

use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{bail, Context, Result};
use duckdb_neo::{connection::Connection, Parameters};
use serde_json::Value;

use crate::metadata::tushare_fields::{dataset, WriteStrategy};

use super::schema::{
    date_literal, execute_script, ident, sql_string, table_columns, table_keys,
    type_for_table_column,
};

pub fn write_jsonl(
    connection: &Connection,
    table: &str,
    input: &Path,
    strategy: &str,
    dates: &[String],
    scope: &BTreeMap<String, String>,
    key_values: &[Vec<Value>],
) -> Result<i64> {
    if !matches!(
        strategy,
        "append" | "replace-dates" | "replace-keys" | "replace-batch-keys" | "replace-all"
    ) {
        bail!("unknown DuckDB write strategy: {strategy}");
    }
    if strategy == "replace-keys" && key_values.is_empty() {
        bail!("replace-keys requires at least one logical key");
    }
    let cols = table_columns(table)?;
    if strategy == "replace-all"
        && !dataset(table).is_some_and(|metadata| {
            matches!(metadata.write_strategy, WriteStrategy::ReplaceAll { .. })
        })
    {
        bail!("replace-all is only allowed for reference snapshot tables");
    }
    let key_cols = table_keys(table)?;
    for column in scope.keys() {
        if !cols.iter().any(|known| known == column) {
            bail!("scope column is not part of {table}: {column}");
        }
    }
    let stage = "__stock_cache_stage";
    execute_script(connection, &format!("DROP TABLE IF EXISTS {stage}; CREATE TEMP TABLE {stage} AS SELECT * FROM {} LIMIT 0; ALTER TABLE {stage} ADD COLUMN __stock_cache_batch_order BIGINT", ident(table)))?;
    let metadata =
        fs::metadata(input).with_context(|| format!("stat JSONL {}", input.display()))?;
    if metadata.len() > 0 {
        let projection = cols
            .iter()
            .map(|column| {
                let source = ident(column);
                let target_type = type_for_table_column(table, column);
                if target_type == "DATE" {
                    format!(
                        "COALESCE(try_strptime(CAST({source} AS VARCHAR), '%Y-%m-%d')::DATE, try_strptime(CAST({source} AS VARCHAR), '%Y%m%d')::DATE) AS {source}"
                    )
                } else {
                    format!("CAST({source} AS {target_type}) AS {source}")
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let input_columns = cols
            .iter()
            .map(|column| format!("{}: 'VARCHAR'", sql_string(column)))
            .collect::<Vec<_>>()
            .join(", ");
        let path = sql_string(&input.to_string_lossy());
        let sql = format!("INSERT INTO {stage} ({}, __stock_cache_batch_order) SELECT {projection}, ROW_NUMBER() OVER () - 1 FROM read_json_auto({path}, format='newline_delimited', records=true, columns={{{input_columns}}})", cols.iter().map(|x| ident(x)).collect::<Vec<_>>().join(", "));
        connection.execute(sql, Parameters::None)?;
    }
    let result = (|| -> Result<i64> {
        connection.execute("BEGIN TRANSACTION", Parameters::None)?;
        if strategy == "append" {
            let join = key_cols
                .iter()
                .map(|column| format!("target.{0}=staged.{0}", ident(column)))
                .collect::<Vec<_>>()
                .join(" AND ");
            let exists = format!("SELECT 1 FROM {table} target JOIN (SELECT {keys} FROM {stage} QUALIFY ROW_NUMBER() OVER (PARTITION BY {keys} ORDER BY __stock_cache_batch_order DESC)=1) staged ON {join} LIMIT 1", table=ident(table), keys=key_cols.iter().map(|x| ident(x)).collect::<Vec<_>>().join(", "));
            let mut check = connection.query(exists, Parameters::None)?;
            let already_exists = check.next_chunk()?.is_some();
            drop(check);
            if already_exists {
                bail!("append batch contains a logical key already present in target");
            }
        } else if strategy == "replace-all" {
            connection.execute(format!("DELETE FROM {}", ident(table)), Parameters::None)?;
        } else if strategy == "replace-batch-keys" {
            let key_match = key_cols
                .iter()
                .map(|column| format!("target.{0} IS NOT DISTINCT FROM staged.{0}", ident(column)))
                .collect::<Vec<_>>()
                .join(" AND ");
            let delete = format!(
                "DELETE FROM {} AS target USING (SELECT DISTINCT {} FROM {stage}) AS staged WHERE {key_match}",
                ident(table),
                key_cols
                    .iter()
                    .map(|column| ident(column))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            connection.execute(delete, Parameters::None)?;
        } else {
            let predicate =
                replacement_predicate(table, &cols, dates, scope, key_cols.as_slice(), key_values)?;
            connection.execute(
                format!("DELETE FROM {} WHERE {predicate}", ident(table)),
                Parameters::None,
            )?;
        }
        let key_sql = key_cols
            .iter()
            .map(|x| ident(x))
            .collect::<Vec<_>>()
            .join(", ");
        let count_sql = format!("SELECT COUNT(*)::BIGINT FROM (SELECT ROW_NUMBER() OVER (PARTITION BY {key_sql} ORDER BY __stock_cache_batch_order DESC) rank FROM {stage}) WHERE rank=1");
        let mut count = connection.query(count_sql, Parameters::None)?;
        let written = count
            .next_chunk()?
            .and_then(|chunk| {
                chunk.get_vector_at::<i64>(0).ok().and_then(|v| {
                    v.reader()
                        .ok()
                        .and_then(|r| r.get(0).ok().flatten().copied())
                })
            })
            .unwrap_or(0);
        drop(count);
        let insert = format!("INSERT INTO {} ({}) SELECT {} FROM {stage} QUALIFY ROW_NUMBER() OVER (PARTITION BY {key_sql} ORDER BY __stock_cache_batch_order DESC)=1", ident(table), cols.iter().map(|x| ident(x)).collect::<Vec<_>>().join(", "), cols.iter().map(|x| ident(x)).collect::<Vec<_>>().join(", "));
        connection.execute(insert, Parameters::None)?;
        connection.execute("COMMIT", Parameters::None)?;
        Ok(written)
    })();
    if result.is_err() {
        let _ = connection.execute("ROLLBACK", Parameters::None);
    }
    let _ = connection.execute(format!("DROP TABLE IF EXISTS {stage}"), Parameters::None);
    result
}

fn replacement_predicate(
    table: &str,
    cols: &[String],
    dates: &[String],
    scope: &BTreeMap<String, String>,
    key_cols: &[String],
    key_values: &[Vec<Value>],
) -> Result<String> {
    if !key_values.is_empty() {
        if key_values.iter().any(|key| key_cols.len() != key.len()) {
            bail!("replace key width does not match table key");
        }
        return Ok(key_values
            .iter()
            .map(|key| {
                key.iter()
                    .enumerate()
                    .map(|(index, value)| {
                        format!(
                            "{} = {}",
                            ident(&key_cols[index]),
                            json_literal(value, type_for_table_column(table, &key_cols[index]))
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" AND ")
            })
            .map(|x| format!("({x})"))
            .collect::<Vec<_>>()
            .join(" OR "));
    }
    if dates.is_empty() {
        bail!("replace-dates requires non-empty dates");
    }
    if !cols
        .iter()
        .any(|x| x == "trade_date" || x == "period_end_date")
    {
        bail!("table has no date replacement column");
    }
    let date_column = if cols.iter().any(|x| x == "trade_date") {
        "trade_date"
    } else {
        "period_end_date"
    };
    let date_sql = dates
        .iter()
        .map(|x| date_literal(x))
        .collect::<Result<Vec<_>>>()?
        .join(", ");
    let mut terms = vec![format!("{} IN ({date_sql})", ident(date_column))];
    if table.starts_with("calc_") {
        for required in ["source_dataset", "calc_version"] {
            if !scope.contains_key(required) {
                bail!("calculation replacement requires source_dataset and calc_version scope");
            }
        }
    }
    for (column, value) in scope {
        terms.push(format!(
            "{} IS NOT DISTINCT FROM {}",
            ident(column),
            json_literal(
                &Value::String(value.clone()),
                type_for_table_column(table, column)
            )
        ));
    }
    Ok(terms.join(" AND "))
}

fn json_literal(value: &Value, ty: &str) -> String {
    if value.is_null() {
        "NULL".to_string()
    } else if ty == "DATE" {
        value
            .as_str()
            .and_then(|raw| date_literal(raw).ok())
            .unwrap_or_else(|| "NULL".to_string())
    } else if ty == "BOOLEAN" || ty == "DOUBLE" || ty == "BIGINT" {
        value.to_string()
    } else {
        let text = value
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| value.to_string());
        sql_string(&text)
    }
}
