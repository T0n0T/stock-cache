use assert_cmd::Command;
use predicates::prelude::*;

fn backfill_czsc() -> Command {
    Command::cargo_bin("backfill-czsc-structure").expect("binary exists")
}

#[test]
fn help_exposes_the_dedicated_backfill_controls() {
    backfill_czsc()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--start-date"))
        .stdout(predicate::str::contains("--end-date"))
        .stdout(predicate::str::contains("--max-concurrency"))
        .stdout(predicate::str::contains("--chunk-size"))
        .stdout(predicate::str::contains("--dry-run"))
        .stdout(predicate::str::contains("--force"));
}

#[test]
fn unpaired_dates_are_rejected_before_connecting() {
    backfill_czsc()
        .args(["--start-date", "2026-06-01", "--dry-run"])
        .env("POSTGRES_DSN", "not-a-postgres-dsn")
        .env_remove("TUSHARE_TOKEN")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--start-date and --end-date must be provided together",
        ))
        .stderr(predicate::str::contains("TUSHARE_TOKEN").not());
}

#[test]
fn compact_dates_are_rejected_before_connecting() {
    backfill_czsc()
        .args([
            "--start-date",
            "20260601",
            "--end-date",
            "2026-06-30",
            "--dry-run",
        ])
        .env("POSTGRES_DSN", "not-a-postgres-dsn")
        .env_remove("TUSHARE_TOKEN")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--start-date must use YYYY-MM-DD format",
        ));
}
