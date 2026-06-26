use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::NamedTempFile;

fn stock_cache() -> Command {
    Command::cargo_bin("stock-cache").expect("binary exists")
}

#[test]
fn config_show_reads_env_file_and_hides_secrets() {
    let env_file = NamedTempFile::new().unwrap();
    std::fs::write(
        env_file.path(),
        "POSTGRES_DSN=postgres://env-file\nTUSHARE_TOKEN=secret-token\nMAX_CONCURRENCY=7\n",
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
        .stdout(predicate::str::contains("\"postgres_dsn_set\": true"))
        .stdout(predicate::str::contains("\"tushare_token_set\": true"))
        .stdout(predicate::str::contains("\"max_concurrency\": 7"))
        .stdout(predicate::str::contains("secret-token").not())
        .stdout(predicate::str::contains("postgres://env-file").not());
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
        .stdout(predicate::str::contains("\"max_concurrency\": 9"))
        .stdout(predicate::str::contains("shell-token").not())
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
fn write_calc_still_reports_not_implemented_boundary() {
    stock_cache()
        .args([
            "write",
            "--mode",
            "calc",
            "--start-date",
            "20260601",
            "--end-date",
            "20260601",
        ])
        .env("POSTGRES_DSN", "postgres://example")
        .env("TUSHARE_TOKEN", "token")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not implemented in this phase"));
}
