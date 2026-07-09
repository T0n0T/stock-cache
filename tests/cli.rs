use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::NamedTempFile;

fn stock_cache() -> Command {
    Command::cargo_bin("stock-cache").expect("binary exists")
}

#[test]
fn config_show_reads_env_file_and_prints_effective_values() {
    let env_file = NamedTempFile::new().unwrap();
    std::fs::write(
        env_file.path(),
        "POSTGRES_DSN=postgres://env-file\nTUSHARE_TOKEN=secret-token\nMAX_CONCURRENCY=7\nCACHE_WARMUP_CONCURRENCY=3\nCALC_CHUNK_SIZE=33\n",
    )
    .unwrap();

    stock_cache()
        .arg("--env-file")
        .arg(env_file.path())
        .args(["config", "show"])
        .env_remove("POSTGRES_DSN")
        .env_remove("TUSHARE_TOKEN")
        .assert()
        .success()
        .stdout(predicate::str::contains("Configuration"))
        .stdout(predicate::str::contains("POSTGRES_DSN: set"))
        .stdout(predicate::str::contains("TUSHARE_TOKEN: set"))
        .stdout(predicate::str::contains("MAX_CONCURRENCY: 7"))
        .stdout(predicate::str::contains("CACHE_WARMUP_CONCURRENCY: 3"))
        .stdout(predicate::str::contains("CALC_CHUNK_SIZE: 33"))
        .stdout(predicate::str::contains("postgres://env-file").not())
        .stdout(predicate::str::contains("secret-token").not());
}

#[test]
fn shell_env_overrides_env_file() {
    let env_file = NamedTempFile::new().unwrap();
    std::fs::write(
        env_file.path(),
        "POSTGRES_DSN=postgres://env-file\nTUSHARE_TOKEN=file-token\nMAX_CONCURRENCY=7\n",
    )
    .unwrap();

    stock_cache()
        .arg("--env-file")
        .arg(env_file.path())
        .args(["config", "show"])
        .env("POSTGRES_DSN", "postgres://shell")
        .env("TUSHARE_TOKEN", "shell-token")
        .env("MAX_CONCURRENCY", "9")
        .assert()
        .success()
        .stdout(predicate::str::contains("POSTGRES_DSN: set"))
        .stdout(predicate::str::contains("TUSHARE_TOKEN: set"))
        .stdout(predicate::str::contains("MAX_CONCURRENCY: 9"))
        .stdout(predicate::str::contains("postgres://shell").not())
        .stdout(predicate::str::contains("shell-token").not())
        .stdout(predicate::str::contains("postgres://env-file").not())
        .stdout(predicate::str::contains("file-token").not());
}

#[test]
fn write_rejects_unpaired_dates() {
    stock_cache()
        .args(["write", "--mode", "full", "--start-date", "20260601"])
        .env("POSTGRES_DSN", "postgres://example")
        .env("TUSHARE_TOKEN", "token")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--start-date and --end-date must be provided together",
        ));
}

#[test]
fn write_cache_default_lookback_requires_explicit_dates_for_now() {
    stock_cache()
        .args(["write", "--mode", "cache"])
        .env("POSTGRES_DSN", "postgres://example")
        .env("TUSHARE_TOKEN", "token")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "cache execution currently requires explicit --start-date/--end-date",
        ));
}

#[test]
fn write_calc_rejects_unpaired_dates_before_connecting() {
    stock_cache()
        .args(["write", "--mode", "calc", "--start-date", "20260601"])
        .env("POSTGRES_DSN", "postgres://example")
        .env("TUSHARE_TOKEN", "token")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--start-date and --end-date must be provided together",
        ));
}

#[test]
fn write_help_mentions_force_recalc() {
    stock_cache()
        .args(["write", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--refresh"))
        .stdout(predicate::str::contains("--force-recalc"))
        .stdout(predicate::str::contains("--chunk-size"));
}

#[test]
fn erase_help_mentions_start_and_end_dates() {
    stock_cache()
        .args(["erase", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--start-date"))
        .stdout(predicate::str::contains("--end-date"))
        .stdout(predicate::str::contains("--trade-date").not());
}

#[test]
fn erase_does_not_require_tushare_token() {
    stock_cache()
        .args([
            "erase",
            "--start-date",
            "2026-07-03",
            "--end-date",
            "2026-07-03",
        ])
        .env("POSTGRES_DSN", "not-a-postgres-dsn")
        .env_remove("TUSHARE_TOKEN")
        .assert()
        .failure()
        .stderr(predicate::str::contains("TUSHARE_TOKEN").not());
}

#[test]
fn erase_rejects_invalid_trade_date_before_connecting() {
    stock_cache()
        .args([
            "erase",
            "--start-date",
            "20260703",
            "--end-date",
            "2026-07-03",
        ])
        .env("POSTGRES_DSN", "postgres://example")
        .env_remove("TUSHARE_TOKEN")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--start-date must use YYYY-MM-DD format",
        ));
}

#[test]
fn erase_rejects_unpaired_dates_before_connecting() {
    stock_cache()
        .args(["erase", "--start-date", "2026-07-03"])
        .env("POSTGRES_DSN", "postgres://example")
        .env_remove("TUSHARE_TOKEN")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--start-date and --end-date must be provided together",
        ));
}

#[test]
fn write_accepts_legacy_refresh_ths_alias() {
    stock_cache()
        .args(["write", "--mode", "cache", "--refresh-ths"])
        .env("POSTGRES_DSN", "not-a-postgres-dsn")
        .env("TUSHARE_TOKEN", "token")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unexpected argument '--refresh-ths'").not());
}

#[test]
fn contract_check_help_mentions_stock_select_gate_options() {
    stock_cache()
        .args(["contract-check", "stock-select", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--pick-date"))
        .stdout(predicate::str::contains("--lookback-trading-days"))
        .stdout(predicate::str::contains("--period-coverage-threshold"));
}

#[test]
fn status_does_not_require_tushare_token() {
    stock_cache()
        .arg("status")
        .env("POSTGRES_DSN", "not-a-postgres-dsn")
        .env_remove("TUSHARE_TOKEN")
        .assert()
        .failure()
        .stderr(predicate::str::contains("TUSHARE_TOKEN").not());
}

#[test]
fn write_calc_does_not_require_tushare_token() {
    stock_cache()
        .args(["write", "--mode", "calc"])
        .env("POSTGRES_DSN", "not-a-postgres-dsn")
        .env_remove("TUSHARE_TOKEN")
        .assert()
        .failure()
        .stderr(predicate::str::contains("TUSHARE_TOKEN").not());
}

#[test]
fn contract_check_stock_select_does_not_require_tushare_token() {
    stock_cache()
        .args(["contract-check", "stock-select"])
        .env("POSTGRES_DSN", "not-a-postgres-dsn")
        .env_remove("TUSHARE_TOKEN")
        .assert()
        .failure()
        .stderr(predicate::str::contains("TUSHARE_TOKEN").not());
}
