use std::collections::BTreeMap;

use anyhow::{Context, Result};
use chrono::{Duration as ChronoDuration, NaiveDate};
use serde::Serialize;
use tokio_postgres::{Client, NoTls};

use crate::{app::config::Config, cli::EraseArgs, source::writer::parse_date_text};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct EraseTarget {
    pub table: &'static str,
    pub date_column: &'static str,
}

const ERASE_TARGETS: &[EraseTarget] = &[
    EraseTarget {
        table: "stock_daily_left_peak",
        date_column: "trade_date",
    },
    EraseTarget {
        table: "stock_daily_rolling_factors",
        date_column: "trade_date",
    },
    EraseTarget {
        table: "stock_daily_asof_indicators",
        date_column: "trade_date",
    },
    EraseTarget {
        table: "index_daily_asof_indicators",
        date_column: "trade_date",
    },
    EraseTarget {
        table: "stock_period_indicator_state",
        date_column: "period_end_date",
    },
    EraseTarget {
        table: "index_period_indicator_state",
        date_column: "period_end_date",
    },
    EraseTarget {
        table: "stock_stk_factor_pro",
        date_column: "trade_date",
    },
    EraseTarget {
        table: "stock_moneyflow_ths",
        date_column: "trade_date",
    },
    EraseTarget {
        table: "stock_limit_list_ths",
        date_column: "trade_date",
    },
    EraseTarget {
        table: "index_limit_cpt_list",
        date_column: "trade_date",
    },
    EraseTarget {
        table: "index_idx_factor_pro",
        date_column: "trade_date",
    },
    EraseTarget {
        table: "index_ths_daily",
        date_column: "trade_date",
    },
    EraseTarget {
        table: "index_moneyflow_cnt_ths",
        date_column: "trade_date",
    },
];

#[derive(Debug, Serialize)]
struct EraseSummary {
    status: &'static str,
    mode: &'static str,
    start_date: String,
    end_date: String,
    target_dates: Vec<String>,
    deleted_row_counts: BTreeMap<String, u64>,
    message: &'static str,
}

pub async fn run(args: EraseArgs, config: &Config) -> Result<()> {
    let target_dates = resolve_erase_dates(&args)?;
    let (mut client, connection) = tokio_postgres::connect(&config.postgres_dsn, NoTls).await?;
    let connection_task = tokio::spawn(async move {
        if let Err(error) = connection.await {
            eprintln!("postgres connection error: {error}");
        }
    });

    let deleted_row_counts = erase_dates(&mut client, &target_dates).await?;
    connection_task.abort();
    let start_date = target_dates
        .first()
        .expect("checked non-empty target dates");
    let end_date = target_dates.last().expect("checked non-empty target dates");
    let summary = EraseSummary {
        status: "success",
        mode: "erase",
        start_date: start_date.format("%Y-%m-%d").to_string(),
        end_date: end_date.format("%Y-%m-%d").to_string(),
        target_dates: target_dates
            .iter()
            .map(|date| date.format("%Y-%m-%d").to_string())
            .collect(),
        deleted_row_counts,
        message: "date window data erased",
    };
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

fn resolve_erase_dates(args: &EraseArgs) -> Result<Vec<NaiveDate>> {
    let has_start = args.start_date.is_some();
    let has_end = args.end_date.is_some();
    if has_start != has_end {
        anyhow::bail!("--start-date and --end-date must be provided together");
    }
    let start_text = args
        .start_date
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("--start-date and --end-date must be provided together"))?;
    let end_text = args
        .end_date
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("--start-date and --end-date must be provided together"))?;
    let start = parse_erase_date_arg("--start-date", start_text)?;
    let end = parse_erase_date_arg("--end-date", end_text)?;
    if start > end {
        anyhow::bail!("--start-date must be <= --end-date");
    }

    let mut dates = Vec::new();
    let mut current = start;
    while current <= end {
        dates.push(current);
        current = current
            .checked_add_signed(ChronoDuration::days(1))
            .ok_or_else(|| anyhow::anyhow!("failed to calculate next erase date"))?;
    }
    Ok(dates)
}

fn parse_erase_date_arg(name: &str, value: &str) -> Result<NaiveDate> {
    let compact = value.len() == 8 && value.bytes().all(|byte| byte.is_ascii_digit());
    let dashed = value.len() == 10
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value
            .bytes()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit());
    if !compact && !dashed {
        anyhow::bail!("{name} must use YYYYMMDD or YYYY-MM-DD format");
    }
    parse_date_text(value).with_context(|| format!("{name} must use YYYYMMDD or YYYY-MM-DD format"))
}

async fn erase_dates(
    client: &mut Client,
    target_dates: &[NaiveDate],
) -> Result<BTreeMap<String, u64>> {
    let transaction = client.transaction().await?;
    let mut deleted_row_counts = BTreeMap::new();
    for trade_date in target_dates {
        for target in ERASE_TARGETS {
            let sql = delete_sql(target);
            let deleted = transaction.execute(sql.as_str(), &[trade_date]).await?;
            *deleted_row_counts
                .entry(target.table.to_string())
                .or_insert(0) += deleted;
        }
    }
    transaction.commit().await?;
    Ok(deleted_row_counts)
}

fn delete_sql(target: &EraseTarget) -> String {
    format!(
        r#"delete from "{}" where "{}" = $1"#,
        target.table, target.date_column
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erase_targets_include_source_and_calc_date_slices_only() {
        let targets = ERASE_TARGETS
            .iter()
            .map(|target| (target.table, target.date_column))
            .collect::<Vec<_>>();

        assert_eq!(
            targets,
            vec![
                ("stock_daily_left_peak", "trade_date"),
                ("stock_daily_rolling_factors", "trade_date"),
                ("stock_daily_asof_indicators", "trade_date"),
                ("index_daily_asof_indicators", "trade_date"),
                ("stock_period_indicator_state", "period_end_date"),
                ("index_period_indicator_state", "period_end_date"),
                ("stock_stk_factor_pro", "trade_date"),
                ("stock_moneyflow_ths", "trade_date"),
                ("stock_limit_list_ths", "trade_date"),
                ("index_limit_cpt_list", "trade_date"),
                ("index_idx_factor_pro", "trade_date"),
                ("index_ths_daily", "trade_date"),
                ("index_moneyflow_cnt_ths", "trade_date"),
            ]
        );
        assert!(!targets.iter().any(|(table, _)| *table == "stock_basic"));
        assert!(!targets.iter().any(|(table, _)| *table == "index_ths"));
        assert!(!targets
            .iter()
            .any(|(table, _)| *table == "index_ths_member"));
    }

    #[test]
    fn erase_accepts_compact_and_dashed_date_window() {
        let args = EraseArgs {
            start_date: Some("20260703".to_string()),
            end_date: Some("2026-07-04".to_string()),
        };
        assert_eq!(
            resolve_erase_dates(&args).unwrap(),
            vec![
                NaiveDate::from_ymd_opt(2026, 7, 3).unwrap(),
                NaiveDate::from_ymd_opt(2026, 7, 4).unwrap()
            ]
        );
    }

    #[test]
    fn erase_rejects_non_zero_padded_trade_date() {
        assert!(parse_erase_date_arg("--start-date", "2026-7-3")
            .unwrap_err()
            .to_string()
            .contains("YYYYMMDD or YYYY-MM-DD"));
    }

    #[test]
    fn erase_dates_must_be_paired() {
        let args = EraseArgs {
            start_date: Some("20260703".to_string()),
            end_date: None,
        };
        assert!(resolve_erase_dates(&args)
            .unwrap_err()
            .to_string()
            .contains("provided together"));
    }

    #[test]
    fn erase_start_date_must_not_be_after_end_date() {
        let args = EraseArgs {
            start_date: Some("20260704".to_string()),
            end_date: Some("20260703".to_string()),
        };
        assert!(resolve_erase_dates(&args)
            .unwrap_err()
            .to_string()
            .contains("--start-date must be <= --end-date"));
    }

    #[test]
    fn delete_sql_quotes_table_and_date_column() {
        assert_eq!(
            delete_sql(&EraseTarget {
                table: "stock_period_indicator_state",
                date_column: "period_end_date",
            }),
            r#"delete from "stock_period_indicator_state" where "period_end_date" = $1"#
        );
    }
}
