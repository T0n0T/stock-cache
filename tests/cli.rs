use std::{fs::File, io::Write, path::Path, process::Output};

use assert_cmd::Command;
use chrono::{Datelike, Duration, NaiveDate};
use serde_json::{json, Value};
use stock_cache::v3::duckdb;
use tempfile::tempdir;

fn run(binary: &str, args: &[&str]) -> Output {
    let output = Command::cargo_bin(binary)
        .unwrap()
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{binary} {args:?} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn run_json(binary: &str, args: &[&str]) -> Value {
    let output = run(binary, args);
    serde_json::from_slice(&output.stdout).unwrap()
}

fn write_jsonl(path: &Path, rows: &[Value]) {
    let mut file = File::create(path).unwrap();
    for row in rows {
        serde_json::to_writer(&mut file, row).unwrap();
        file.write_all(b"\n").unwrap();
    }
}

fn source_row(trade_date: &str, close: f64) -> Value {
    json!({
        "source_dataset": "source_daily_bars",
        "ts_code": "000001.SZ",
        "trade_date": trade_date,
        "open": close - 0.5,
        "high": close + 0.5,
        "low": close - 1.0,
        "close": close,
        "vol": 100.0,
        "amount": 1000.0,
        "price_basis": "qfq"
    })
}

#[test]
fn v3_db_cli_is_idempotent_and_applies_source_write_strategies() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("stock-cache.duckdb");
    let database = database.to_str().unwrap();

    assert_eq!(
        run_json("stock-cache-db", &["init", "--database", database])["status"],
        "ok"
    );
    assert_eq!(
        run_json("stock-cache-db", &["status", "--database", database])["tables"]
            ["source_daily_bars"],
        0
    );

    let source = directory.path().join("source.jsonl");
    write_jsonl(
        &source,
        &[
            source_row("2025-01-02", 10.0),
            source_row("2025-01-03", 11.0),
        ],
    );
    assert_eq!(
        run_json(
            "stock-cache-db",
            &[
                "write",
                "--database",
                database,
                "--table",
                "source_daily_bars",
                "--input",
                source.to_str().unwrap(),
                "--strategy",
                "append",
            ],
        )["written_rows"],
        2
    );

    let replacement = directory.path().join("replacement.jsonl");
    write_jsonl(&replacement, &[source_row("2025-01-02", 99.0)]);
    run(
        "stock-cache-db",
        &[
            "write",
            "--database",
            database,
            "--table",
            "source_daily_bars",
            "--input",
            replacement.to_str().unwrap(),
            "--strategy",
            "replace-dates",
            "--dates",
            "2025-01-02",
        ],
    );

    duckdb::with_connection(database, |connection| {
        let mut query = connection.query(
            "SELECT close, turnover_rate FROM source_daily_bars WHERE trade_date = DATE '2025-01-02'",
            duckdb_neo::Parameters::None,
        )?;
        let chunk = query.next_chunk()?.unwrap();
        assert_eq!(
            chunk
                .get_vector_at::<f64>(0)?
                .reader()?
                .get(0)?
                .copied(),
            Some(99.0)
        );
        assert_eq!(
            chunk
                .get_vector_at::<f64>(1)?
                .reader()?
                .get(0)?
                .copied(),
            None
        );
        Ok(())
    })
    .unwrap();

    let empty = directory.path().join("empty.jsonl");
    File::create(&empty).unwrap();
    run(
        "stock-cache-db",
        &[
            "write",
            "--database",
            database,
            "--table",
            "source_daily_bars",
            "--input",
            empty.to_str().unwrap(),
            "--strategy",
            "replace-dates",
            "--dates",
            "2025-01-02",
        ],
    );
    run(
        "stock-cache-db",
        &[
            "write",
            "--database",
            database,
            "--table",
            "source_daily_bars",
            "--input",
            empty.to_str().unwrap(),
            "--strategy",
            "replace-keys",
            "--key-values",
            "[[\"source_daily_bars\",\"000001.SZ\",\"2025-01-03\"]]",
        ],
    );
    assert_eq!(
        run_json("stock-cache-db", &["status", "--database", database])["tables"]
            ["source_daily_bars"],
        0
    );
}

#[test]
fn v3_calc_cli_persists_calculation_tables() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("stock-cache.duckdb");
    let bars_path = directory.path().join("bars.jsonl");
    let calendar_path = directory.path().join("calendar.txt");
    let output_path = directory.path().join("calculated.jsonl");
    let start = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
    let dates = (0..35)
        .map(|offset| start + Duration::days(offset))
        .filter(|day| day.weekday().number_from_monday() <= 5)
        .collect::<Vec<_>>();
    let rows = dates
        .iter()
        .enumerate()
        .map(|(index, day)| {
            json!({
                "source_dataset": "demo",
                "ts_code": "000001.SZ",
                "trade_date": day.to_string(),
                "open": 9.5 + index as f64,
                "high": 10.5 + index as f64,
                "low": 9.0 + index as f64,
                "close": 10.0 + index as f64,
                "vol": 100.0,
                "amount": 1000.0
            })
        })
        .collect::<Vec<_>>();
    write_jsonl(&bars_path, &rows);
    std::fs::write(
        &calendar_path,
        dates
            .iter()
            .map(|day| format!("{day}\n"))
            .collect::<String>(),
    )
    .unwrap();

    run(
        "stock-cache-calc",
        &[
            "--input",
            bars_path.to_str().unwrap(),
            "--calendar",
            calendar_path.to_str().unwrap(),
            "--database",
            database.to_str().unwrap(),
            "--write",
            "--output",
            output_path.to_str().unwrap(),
            "--calc-version",
            "v3-test",
        ],
    );

    let output_rows = std::fs::read_to_string(output_path)
        .unwrap()
        .lines()
        .count();
    assert!(output_rows > dates.len());
    duckdb::with_connection(database.to_str().unwrap(), |connection| {
        for table in [
            "calc_daily_indicators",
            "calc_daily_czsc_structure",
            "calc_daily_rolling_factors",
            "calc_daily_left_peak",
        ] {
            assert_eq!(duckdb::table_count(connection, table)?, dates.len() as i64);
        }
        let mut daily_states = connection.query(
            "SELECT COUNT(*)::BIGINT FROM calc_indicator_state WHERE period_type = 'daily'",
            duckdb_neo::Parameters::None,
        )?;
        let daily_state_count = daily_states
            .next_chunk()?
            .unwrap()
            .get_vector_at::<i64>(0)?
            .reader()?
            .get(0)?
            .copied()
            .unwrap();
        drop(daily_states);
        assert_eq!(daily_state_count, dates.len() as i64);
        assert!(duckdb::table_count(connection, "calc_period_bars")? > 0);
        Ok(())
    })
    .unwrap();
}
