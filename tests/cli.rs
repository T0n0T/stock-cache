use assert_cmd::Command;
use serde_json::Value;
use tempfile::tempdir;

#[test]
fn init_db_and_status_use_dotenv_database_without_database_argument() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("runtime/cache.duckdb");
    std::fs::write(
        directory.path().join(".env"),
        format!("DUCKDB_PATH='{}'\n", database.display()),
    )
    .unwrap();
    for command in ["init-db", "status"] {
        let output = Command::cargo_bin("stock-cache")
            .unwrap()
            .current_dir(directory.path())
            .env_remove("DUCKDB_PATH")
            .arg(command)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let summary: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(summary["database"], database.to_str().unwrap());
    }
    assert!(database.is_file());
}

#[test]
fn erase_removes_only_requested_dates_and_invalidates_coverage() {
    use duckdb_neo::Parameters;
    use stock_cache::{metadata::tushare_fields::DATASETS, storage::duckdb};
    let directory = tempdir().unwrap();
    let database = directory.path().join("cache.duckdb");
    let database = database.to_str().unwrap();
    let mut tables = DATASETS
        .iter()
        .filter(|d| d.field("trade_date").is_some())
        .map(|d| (d.table, "trade_date"))
        .collect::<Vec<_>>();
    tables.extend([
        ("source_daily_bars", "trade_date"),
        ("calc_period_bars", "period_end_date"),
        ("calc_indicator_state", "period_end_date"),
        ("calc_daily_indicators", "trade_date"),
        ("calc_daily_czsc_structure", "trade_date"),
        ("calc_daily_rolling_factors", "trade_date"),
        ("calc_daily_left_peak", "trade_date"),
        ("cache_fetch_coverage", "trade_date"),
    ]);
    duckdb::with_connection(database, |connection| {
        duckdb::create_schema(connection)?;
        for (table, date_column) in &tables {
            for day in ["2026-11-09", "2026-11-10", "2026-11-11"] {
                let (columns, values) = if *table == "cache_fetch_coverage" {
                    (
                        "dataset,trade_date,code,row_count".to_string(),
                        format!("'stock_stk_factor_pro',DATE '{day}','',1"),
                    )
                } else if table.starts_with("calc_") {
                    let period = if *date_column == "period_end_date" {
                        format!(",'daily',DATE '{day}'")
                    } else {
                        String::new()
                    };
                    let price = if *table == "calc_period_bars" {
                        ",'qfq'"
                    } else {
                        ""
                    };
                    (
                        format!(
                            "source_dataset,ts_code,{date_column},calc_version{}{}",
                            if period.is_empty() {
                                ""
                            } else {
                                ",period_type,period_start_date"
                            },
                            if price.is_empty() { "" } else { ",price_basis" }
                        ),
                        format!(
                            "'stock_stk_factor_pro','000001.SZ',DATE '{day}','test'{period}{price}"
                        ),
                    )
                } else if *table == "source_daily_bars" {
                    (
                        "source_dataset,ts_code,trade_date,price_basis".into(),
                        format!("'demo','000001.SZ',DATE '{day}','qfq'"),
                    )
                } else if *table == "index_limit_cpt_list" {
                    (
                        "ts_code,trade_date,rank".into(),
                        format!("'000001.SZ',DATE '{day}',1"),
                    )
                } else {
                    (
                        "ts_code,trade_date".into(),
                        format!("'000001.SZ',DATE '{day}'"),
                    )
                };
                connection.execute(
                    format!("INSERT INTO {table} ({columns}) VALUES ({values})"),
                    Parameters::None,
                )?;
            }
        }
        for table in ["stock_basic", "index_ths"] {
            connection.execute(
                format!("INSERT INTO {table} (ts_code) VALUES ('demo')"),
                Parameters::None,
            )?;
        }
        connection.execute(
            "INSERT INTO index_ths_member (ts_code,con_code) VALUES ('demo','member')",
            Parameters::None,
        )?;
        connection.execute(
            "INSERT INTO cache_trade_calendar VALUES (DATE '2026-11-10')",
            Parameters::None,
        )?;
        Ok(())
    })
    .unwrap();
    let invoke = |args: &[&str]| {
        Command::cargo_bin("stock-cache")
            .unwrap()
            .args(args)
            .args(["--database", database])
            .output()
            .unwrap()
    };
    for args in [
        vec!["erase"],
        vec!["erase", "--start-date", "2026-11-09"],
        vec![
            "erase",
            "--start-date",
            "20261109",
            "--end-date",
            "2026-11-10",
        ],
        vec![
            "erase",
            "--start-date",
            "2026-11-11",
            "--end-date",
            "2026-11-09",
        ],
    ] {
        assert!(!invoke(&args).status.success());
    }
    let args = [
        "erase",
        "--start-date",
        "2026-11-10",
        "--end-date",
        "2026-11-10",
    ];
    let output = invoke(&args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: Value = serde_json::from_slice(&output.stdout).unwrap();
    for (table, _) in &tables {
        assert_eq!(summary["deleted_row_counts"][table], 1);
    }
    duckdb::with_connection(database, |connection| {
        for (table, _) in &tables {
            assert_eq!(duckdb::table_count(connection, table)?, 2, "{table}");
        }
        for table in [
            "stock_basic",
            "index_ths",
            "index_ths_member",
            "cache_trade_calendar",
        ] {
            assert_eq!(duckdb::table_count(connection, table)?, 1);
        }
        Ok(())
    })
    .unwrap();
    let retry = invoke(&args);
    assert!(retry.status.success());
    let retry: Value = serde_json::from_slice(&retry.stdout).unwrap();
    assert!(retry["deleted_row_counts"]
        .as_object()
        .unwrap()
        .values()
        .all(|v| v == 0));
    for command in ["import", "fetch", "calc"] {
        assert!(!Command::cargo_bin("stock-cache")
            .unwrap()
            .arg(command)
            .output()
            .unwrap()
            .status
            .success());
    }
}
