#![allow(dead_code)]

use std::collections::{BTreeMap, HashSet};

use anyhow::{anyhow, Context, Result};
use chrono::{NaiveDate, NaiveDateTime};
use serde_json::Value;
use tokio_postgres::{types::ToSql, Client, Transaction};

use crate::generated::tushare_fields::{DatasetMetadata, FieldMetadata};

use super::write_sql::{static_write_sql, WriteSqlPlan};

pub type SourceRow = BTreeMap<String, Value>;

pub async fn write_dataset(
    client: &mut Client,
    dataset: &DatasetMetadata,
    rows: &[SourceRow],
    delete_date: Option<NaiveDate>,
) -> Result<usize> {
    match static_write_sql(dataset) {
        WriteSqlPlan::Upsert { sql } => upsert_rows(client, dataset, sql, rows).await,
        WriteSqlPlan::ReplaceAll {
            clear_sql,
            insert_sql,
            ..
        } => replace_all_rows(client, dataset, clear_sql, insert_sql, rows).await,
        WriteSqlPlan::DeleteInsertByDate {
            delete_sql,
            insert_sql,
            ..
        } => {
            let date = delete_date.ok_or_else(|| {
                anyhow!(
                    "delete_insert_by_date dataset {} requires a delete date",
                    dataset.id
                )
            })?;
            delete_insert_rows(client, dataset, delete_sql, insert_sql, date, rows).await
        }
    }
}

pub async fn replace_classification_group(
    client: &mut Client,
    index_ths: &DatasetMetadata,
    index_ths_rows: &[SourceRow],
    index_ths_member: &DatasetMetadata,
    index_ths_member_rows: &[SourceRow],
) -> Result<usize> {
    let WriteSqlPlan::ReplaceAll {
        clear_sql: index_ths_clear,
        insert_sql: index_ths_insert,
        transaction_group: index_group,
    } = static_write_sql(index_ths)
    else {
        return Err(anyhow!("{} is not replace_all", index_ths.id));
    };
    let WriteSqlPlan::ReplaceAll {
        clear_sql: index_ths_member_clear,
        insert_sql: index_ths_member_insert,
        transaction_group: member_group,
    } = static_write_sql(index_ths_member)
    else {
        return Err(anyhow!("{} is not replace_all", index_ths_member.id));
    };
    if index_group != Some("ths_classification") || member_group != Some("ths_classification") {
        return Err(anyhow!(
            "THS classification datasets must share transaction group"
        ));
    }

    let transaction = client.transaction().await?;
    transaction.execute(index_ths_member_clear, &[]).await?;
    transaction.execute(index_ths_clear, &[]).await?;
    insert_rows_in_transaction(&transaction, index_ths, index_ths_insert, index_ths_rows).await?;
    insert_rows_in_transaction(
        &transaction,
        index_ths_member,
        index_ths_member_insert,
        index_ths_member_rows,
    )
    .await?;
    transaction.commit().await?;
    Ok(index_ths_rows.len() + index_ths_member_rows.len())
}

async fn upsert_rows(
    client: &Client,
    dataset: &DatasetMetadata,
    sql: &str,
    rows: &[SourceRow],
) -> Result<usize> {
    for row in rows {
        let values = bind_values(dataset, row)?;
        let params = params(&values);
        client.execute(sql, &params).await?;
    }
    Ok(rows.len())
}

async fn replace_all_rows(
    client: &mut Client,
    dataset: &DatasetMetadata,
    clear_sql: &str,
    insert_sql: &str,
    rows: &[SourceRow],
) -> Result<usize> {
    let transaction = client.transaction().await?;
    transaction.execute(clear_sql, &[]).await?;
    insert_rows_in_transaction(&transaction, dataset, insert_sql, rows).await?;
    transaction.commit().await?;
    Ok(rows.len())
}

async fn delete_insert_rows(
    client: &mut Client,
    dataset: &DatasetMetadata,
    delete_sql: &str,
    insert_sql: &str,
    delete_date: NaiveDate,
    rows: &[SourceRow],
) -> Result<usize> {
    let transaction = client.transaction().await?;
    transaction.execute(delete_sql, &[&delete_date]).await?;
    insert_rows_in_transaction(&transaction, dataset, insert_sql, rows).await?;
    transaction.commit().await?;
    Ok(rows.len())
}

async fn insert_rows_in_transaction(
    transaction: &Transaction<'_>,
    dataset: &DatasetMetadata,
    insert_sql: &str,
    rows: &[SourceRow],
) -> Result<()> {
    for row in rows {
        let values = bind_values(dataset, row)?;
        let params = params(&values);
        transaction.execute(insert_sql, &params).await?;
    }
    Ok(())
}

fn params(values: &[SqlValue]) -> Vec<&(dyn ToSql + Sync)> {
    values
        .iter()
        .map(|value| match value {
            SqlValue::Text(value) => value as &(dyn ToSql + Sync),
            SqlValue::Double(value) => value as &(dyn ToSql + Sync),
            SqlValue::Integer(value) => value as &(dyn ToSql + Sync),
            SqlValue::Date(value) => value as &(dyn ToSql + Sync),
            SqlValue::Timestamp(value) => value as &(dyn ToSql + Sync),
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
enum SqlValue {
    Text(Option<String>),
    Double(Option<f64>),
    Integer(Option<i32>),
    Date(Option<NaiveDate>),
    Timestamp(Option<NaiveDateTime>),
}

fn bind_values(dataset: &DatasetMetadata, row: &SourceRow) -> Result<Vec<SqlValue>> {
    reject_unknown_fields(dataset, row)?;
    dataset
        .fields
        .iter()
        .map(|field| bind_field(dataset, field, row.get(field.name)))
        .collect()
}

fn reject_unknown_fields(dataset: &DatasetMetadata, row: &SourceRow) -> Result<()> {
    let known = dataset
        .fields
        .iter()
        .map(|field| field.name)
        .collect::<HashSet<_>>();
    for key in row.keys() {
        if !known.contains(key.as_str()) {
            return Err(anyhow!("{} returned undeclared field {key}", dataset.id));
        }
    }
    Ok(())
}

fn bind_field(
    dataset: &DatasetMetadata,
    field: &FieldMetadata,
    value: Option<&Value>,
) -> Result<SqlValue> {
    if !field.nullable && value.map_or(true, is_json_null_or_empty) {
        return Err(anyhow!(
            "{}.{} is required but missing",
            dataset.id,
            field.name
        ));
    }
    match field.pg_type {
        "text" => Ok(SqlValue::Text(parse_text(value))),
        "double precision" => parse_double(value).map(SqlValue::Double).with_context(|| {
            format!(
                "failed to parse {}.{} as double precision",
                dataset.id, field.name
            )
        }),
        "integer" => parse_integer(value)
            .map(SqlValue::Integer)
            .with_context(|| format!("failed to parse {}.{} as integer", dataset.id, field.name)),
        "date" => parse_date(value)
            .map(SqlValue::Date)
            .with_context(|| format!("failed to parse {}.{} as date", dataset.id, field.name)),
        "timestamp without time zone" => parse_timestamp(value)
            .map(SqlValue::Timestamp)
            .with_context(|| format!("failed to parse {}.{} as timestamp", dataset.id, field.name)),
        other => Err(anyhow!("unsupported PostgreSQL type {other}")),
    }
}

fn is_json_null_or_empty(value: &Value) -> bool {
    value.is_null() || matches!(value, Value::String(text) if text.trim().is_empty())
}

fn parse_text(value: Option<&Value>) -> Option<String> {
    match value {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) if text.trim().is_empty() => None,
        Some(Value::String(text)) => Some(text.clone()),
        Some(Value::Number(number)) => Some(number.to_string()),
        Some(Value::Bool(value)) => Some(value.to_string()),
        Some(other) => Some(other.to_string()),
    }
}

fn parse_double(value: Option<&Value>) -> Result<Option<f64>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) if text.trim().is_empty() => Ok(None),
        Some(Value::Number(number)) => number
            .as_f64()
            .map(Some)
            .ok_or_else(|| anyhow!("number is not representable as f64")),
        Some(Value::String(text)) => text.trim().parse::<f64>().map(Some).map_err(Into::into),
        Some(other) => Err(anyhow!("expected number or string, got {other}")),
    }
}

fn parse_integer(value: Option<&Value>) -> Result<Option<i32>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) if text.trim().is_empty() => Ok(None),
        Some(Value::Number(number)) => {
            if let Some(value) = number.as_i64() {
                return i32::try_from(value).map(Some).map_err(Into::into);
            }
            if let Some(value) = number.as_u64() {
                return i32::try_from(value).map(Some).map_err(Into::into);
            }
            if let Some(value) = number.as_f64() {
                return integral_f64_to_i32(value).map(Some);
            }
            Err(anyhow!("number is not an integer"))
        }
        Some(Value::String(text)) => parse_integer_text(text.trim()).map(Some),
        Some(other) => Err(anyhow!("expected integer or string, got {other}")),
    }
}

fn parse_integer_text(text: &str) -> Result<i32> {
    match text.parse::<i32>() {
        Ok(value) => Ok(value),
        Err(integer_error) => {
            let float_value = text.parse::<f64>().map_err(|_| integer_error)?;
            integral_f64_to_i32(float_value)
        }
    }
}

fn integral_f64_to_i32(value: f64) -> Result<i32> {
    if !value.is_finite() {
        return Err(anyhow!("number is not finite"));
    }
    if value.fract() != 0.0 {
        return Err(anyhow!("number is not an integer"));
    }
    if value < i32::MIN as f64 || value > i32::MAX as f64 {
        return Err(anyhow!("number is outside i32 range"));
    }
    Ok(value as i32)
}

fn parse_date(value: Option<&Value>) -> Result<Option<NaiveDate>> {
    let Some(text) = scalar_text(value) else {
        return Ok(None);
    };
    parse_date_text(&text).map(Some)
}

fn parse_timestamp(value: Option<&Value>) -> Result<Option<NaiveDateTime>> {
    let Some(text) = scalar_text(value) else {
        return Ok(None);
    };
    NaiveDateTime::parse_from_str(&text, "%Y-%m-%d %H:%M:%S")
        .map(Some)
        .map_err(Into::into)
}

fn scalar_text(value: Option<&Value>) -> Option<String> {
    match value {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) if text.trim().is_empty() => None,
        Some(Value::String(text)) => Some(text.trim().to_string()),
        Some(Value::Number(number)) => Some(number.to_string()),
        _ => None,
    }
}

pub fn parse_date_text(text: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(text, "%Y%m%d")
        .or_else(|_| NaiveDate::parse_from_str(text, "%Y-%m-%d"))
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generated::tushare_fields::dataset;
    use serde_json::json;

    #[test]
    fn rejects_unknown_fields() {
        let dataset = dataset("index_limit_cpt_list").unwrap();
        let row = BTreeMap::from([("unknown".to_string(), json!(1))]);
        assert!(bind_values(dataset, &row)
            .unwrap_err()
            .to_string()
            .contains("undeclared"));
    }

    #[test]
    fn parses_limit_cpt_values_in_metadata_order() {
        let dataset = dataset("index_limit_cpt_list").unwrap();
        let row = BTreeMap::from([
            ("ts_code".to_string(), json!("885001.TI")),
            ("name".to_string(), json!("fixture")),
            ("trade_date".to_string(), json!("20260625")),
            ("days".to_string(), json!(1)),
            ("up_stat".to_string(), json!("1/1")),
            ("cons_nums".to_string(), json!(2)),
            ("up_nums".to_string(), json!(3)),
            ("pct_chg".to_string(), json!(1.23)),
            ("rank".to_string(), json!(4)),
        ]);

        let values = bind_values(dataset, &row).unwrap();

        assert!(matches!(values[0], SqlValue::Text(Some(_))));
        assert_eq!(
            values[2],
            SqlValue::Date(Some(parse_date_text("20260625").unwrap()))
        );
        assert_eq!(values[8], SqlValue::Integer(Some(4)));
    }

    #[test]
    fn parses_integral_float_numbers_for_integer_fields() {
        assert_eq!(parse_integer(Some(&json!(3.0))).unwrap(), Some(3));
        assert_eq!(parse_integer(Some(&json!("4.0"))).unwrap(), Some(4));
        assert!(parse_integer(Some(&json!(3.5))).is_err());
    }
}
