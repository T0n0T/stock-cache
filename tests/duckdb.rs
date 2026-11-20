use std::{collections::BTreeMap, fs::File, io::Write};

use duckdb_neo::Parameters;
use serde_json::json;
use stock_cache::storage::duckdb;
use tempfile::tempdir;

fn jsonl_file(
    directory: &std::path::Path,
    name: &str,
    rows: &[serde_json::Value],
) -> std::path::PathBuf {
    let path = directory.join(name);
    let mut file = File::create(&path).unwrap();
    for row in rows {
        serde_json::to_writer(&mut file, row).unwrap();
        file.write_all(b"\n").unwrap();
    }
    path
}

fn indicator_row(value: serde_json::Value) -> serde_json::Value {
    let mut row = serde_json::Map::new();
    for column in duckdb::table_columns("calc_daily_indicators").unwrap() {
        row.insert(column, serde_json::Value::Null);
    }
    if let serde_json::Value::Object(values) = value {
        row.extend(values);
    }
    serde_json::Value::Object(row)
}

#[test]
fn create_schema_removes_legacy_source_daily_bars_table() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("cache.duckdb");
    duckdb::with_connection(database.to_str().unwrap(), |connection| {
        connection.execute(
            "CREATE TABLE source_daily_bars (trade_date DATE)",
            Parameters::None,
        )?;

        duckdb::create_schema(connection)?;

        let mut result = connection.query(
            "SELECT COUNT(*)::BIGINT FROM information_schema.tables \
             WHERE table_name = 'source_daily_bars'",
            Parameters::None,
        )?;
        let count = result
            .next_chunk()?
            .unwrap()
            .get_vector_at::<i64>(0)?
            .reader()?
            .get(0)?
            .copied()
            .unwrap();
        assert_eq!(count, 0);
        Ok(())
    })
    .unwrap();
}

#[test]
fn calc_replace_requires_scope_and_rolls_back_bad_batch() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("cache.duckdb");
    let row = indicator_row(json!({
        "source_dataset": "dataset-a",
        "ts_code": "000001.SZ",
        "trade_date": "2025-01-02",
        "daily_macd_period_count": 1,
        "calc_version": "v3-test"
    }));
    let input = jsonl_file(directory.path(), "indicator.jsonl", &[row]);
    duckdb::with_connection(database.to_str().unwrap(), |connection| {
        duckdb::create_schema(connection)?;
        assert_eq!(
            duckdb::write_jsonl(
                connection,
                "calc_daily_indicators",
                &input,
                "append",
                &[],
                &BTreeMap::new(),
                &[],
            )?,
            1
        );
        Ok(())
    })
    .unwrap();

    let missing_scope = duckdb::with_connection(database.to_str().unwrap(), |connection| {
        let empty = jsonl_file(directory.path(), "empty-calc.jsonl", &[]);
        duckdb::write_jsonl(
            connection,
            "calc_daily_indicators",
            &empty,
            "replace-dates",
            &["2025-01-02".to_string()],
            &BTreeMap::from([(String::from("source_dataset"), String::from("dataset-a"))]),
            &[],
        )
    });
    assert!(missing_scope.is_err());

    let invalid = jsonl_file(
        directory.path(),
        "invalid.jsonl",
        &[indicator_row(json!({
            "source_dataset": "dataset-a",
            "ts_code": "000001.SZ",
            "trade_date": "2025-01-02",
            "daily_macd_period_count": -1,
            "calc_version": "v3-test"
        }))],
    );
    let mut scope = BTreeMap::new();
    scope.insert("source_dataset".to_string(), "dataset-a".to_string());
    scope.insert("calc_version".to_string(), "v3-test".to_string());
    let invalid_result = duckdb::with_connection(database.to_str().unwrap(), |connection| {
        duckdb::write_jsonl(
            connection,
            "calc_daily_indicators",
            &invalid,
            "replace-dates",
            &["2025-01-02".to_string()],
            &scope,
            &[],
        )
    });
    assert!(invalid_result.is_err());

    duckdb::with_connection(database.to_str().unwrap(), |connection| {
        assert_eq!(duckdb::table_count(connection, "calc_daily_indicators")?, 1);
        Ok(())
    })
    .unwrap();
}

#[test]
fn batch_key_replacement_only_replaces_staged_instrument_keys() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("cache.duckdb");
    let make_row = |ts_code: &str, period_count: i64| {
        indicator_row(json!({
            "source_dataset": "dataset-a",
            "ts_code": ts_code,
            "trade_date": "2025-01-02",
            "daily_macd_period_count": period_count,
            "calc_version": "v3-test"
        }))
    };
    let initial = jsonl_file(
        directory.path(),
        "initial.jsonl",
        &[make_row("000001.SZ", 1), make_row("000002.SZ", 1)],
    );
    let replacement = jsonl_file(
        directory.path(),
        "replacement.jsonl",
        &[make_row("000001.SZ", 2)],
    );

    duckdb::with_connection(database.to_str().unwrap(), |connection| {
        duckdb::create_schema(connection)?;
        assert_eq!(
            duckdb::write_jsonl(
                connection,
                "calc_daily_indicators",
                &initial,
                "append",
                &[],
                &BTreeMap::new(),
                &[],
            )?,
            2
        );
        assert_eq!(
            duckdb::write_jsonl(
                connection,
                "calc_daily_indicators",
                &replacement,
                "replace-batch-keys",
                &[],
                &BTreeMap::new(),
                &[],
            )?,
            1
        );
        assert_eq!(duckdb::table_count(connection, "calc_daily_indicators")?, 2);

        let mut query = connection.query(
            "SELECT daily_macd_period_count FROM calc_daily_indicators WHERE ts_code = '000001.SZ'",
            duckdb_neo::Parameters::None,
        )?;
        let chunk = query.next_chunk()?.unwrap();
        let values = chunk.get_vector_at::<i64>(0)?.reader()?;
        assert_eq!(values.get(0)?.copied(), Some(2));
        Ok(())
    })
    .unwrap();
}

#[test]
fn rust_writer_reads_indicator_checkpoints() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("checkpoint.duckdb");
    let input = jsonl_file(
        directory.path(),
        "state.jsonl",
        &[json!({
            "source_dataset": "demo",
            "ts_code": "000001.SZ",
            "period_type": "daily",
            "period_start_date": "2025-01-02",
            "period_end_date": "2025-01-02",
            "macd_ema12": 10.0,
            "macd_ema26": 9.0,
            "macd_dea": 0.5,
            "macd_period_count": 12,
            "kdj_k": 55.0,
            "kdj_d": 50.0,
            "kdj_period_count": 8,
            "calc_version": "v3-test"
        })],
    );
    duckdb::with_connection(database.to_str().unwrap(), |connection| {
        duckdb::create_schema(connection)?;
        duckdb::write_jsonl(
            connection,
            "calc_indicator_state",
            &input,
            "append",
            &[],
            &BTreeMap::new(),
            &[],
        )?;
        let rows = duckdb::load_indicator_checkpoints(connection, "demo", "000001.SZ", "v3-test")?;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].period_type, "daily");
        assert_eq!(rows[0].period_end_date.to_string(), "2025-01-02");
        assert_eq!(rows[0].macd_period_count, Some(12));
        Ok(())
    })
    .unwrap();
}

#[test]
fn rust_writer_creates_and_writes_generated_tushare_source_table() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("source.duckdb");
    let mut row = serde_json::Map::new();
    for column in duckdb::table_columns("stock_basic").unwrap() {
        row.insert(column, serde_json::Value::Null);
    }
    row.insert("ts_code".to_string(), json!("000001.SZ"));
    let input = jsonl_file(
        directory.path(),
        "stock-basic.jsonl",
        &[serde_json::Value::Object(row)],
    );
    duckdb::with_connection(database.to_str().unwrap(), |connection| {
        duckdb::create_schema(connection)?;
        assert_eq!(
            duckdb::write_jsonl(
                connection,
                "stock_basic",
                &input,
                "append",
                &[],
                &BTreeMap::new(),
                &[],
            )?,
            1
        );
        assert_eq!(duckdb::table_count(connection, "stock_basic")?, 1);
        Ok(())
    })
    .unwrap();
}

#[test]
fn reference_snapshot_replacement_removes_obsolete_keys_and_rolls_back_failure() {
    let directory = tempdir().unwrap();
    let first = jsonl_file(
        directory.path(),
        "snapshot.jsonl",
        &[
            json!({"ts_code":"000001.SZ"}),
            json!({"ts_code":"000002.SZ"}),
        ],
    );
    let next = jsonl_file(
        directory.path(),
        "next.jsonl",
        &[json!({"ts_code":"000003.SZ"})],
    );
    let bad = jsonl_file(directory.path(), "bad.jsonl", &[json!({"ts_code":null})]);
    duckdb::with_connection(":memory:", |connection| {
        duckdb::create_schema(connection)?;
        for path in [&first, &next] {
            duckdb::write_jsonl(
                connection,
                "stock_basic",
                path,
                "replace-all",
                &[],
                &BTreeMap::new(),
                &[],
            )?;
        }
        assert_eq!(duckdb::table_count(connection, "stock_basic")?, 1);
        assert!(duckdb::write_jsonl(
            connection,
            "stock_basic",
            &bad,
            "replace-all",
            &[],
            &BTreeMap::new(),
            &[]
        )
        .is_err());
        assert_eq!(duckdb::table_count(connection, "stock_basic")?, 1);
        Ok(())
    })
    .unwrap();
}
