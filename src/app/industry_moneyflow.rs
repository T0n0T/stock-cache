//! Local SW2021 level-2 industry moneyflow derived from stock moneyflow.
//!
//! The source is Tushare `moneyflow` (trade-size tiers), joined to the
//! `index_sw_member` interval snapshot.  Results live in
//! `calc_daily_industry_moneyflow` and are rebuilt idempotently for a date
//! window, so an interrupted backfill can simply rerun the same window.

use anyhow::{bail, Result};
use chrono::NaiveDate;
use duckdb_neo::{connection::Connection, Parameters};

use stock_cache::storage::duckdb;

use super::market_data::literal;

const CHUNK_DAYS: usize = 30;

fn scalar_i64(connection: &Connection, sql: &str) -> Result<i64> {
    let mut query = connection.query(sql, Parameters::None)?;
    let chunk = query.next_chunk()?.expect("count returns one row");
    let value = chunk
        .get_vector_at::<i64>(0)?
        .reader()?
        .get(0)?
        .copied()
        .unwrap_or(0);
    Ok(value)
}

/// Rebuild the derived industry rows for `days` (sorted target sessions).
/// Existing rows for the same dates and calc version are replaced, so the
/// function is safe to rerun after a partial fetch or calculation failure.
pub(super) fn rebuild(
    connection: &Connection,
    days: &[NaiveDate],
    calc_version: &str,
) -> Result<usize> {
    if days.is_empty() {
        return Ok(0);
    }
    let table = duckdb::INDUSTRY_MONEYFLOW_TABLE;
    let first = days[0];
    let last = *days.last().expect("non-empty day window");
    let source_rows = scalar_i64(
        connection,
        &format!(
            "SELECT count(*)::BIGINT FROM stock_moneyflow \
             WHERE trade_date BETWEEN DATE '{first}' AND DATE '{last}'"
        ),
    )?;
    if source_rows == 0 {
        eprintln!(
            "[write] industry moneyflow: no stock_moneyflow rows in {first}..{last}; skipped"
        );
        return Ok(0);
    }
    let members = scalar_i64(connection, "SELECT count(*)::BIGINT FROM index_sw_member")?;
    if members == 0 {
        bail!(
            "stock_moneyflow has rows but index_sw_member is empty; \
             run write --mode cache --refresh first"
        );
    }

    let mut written = 0usize;
    for chunk in days.chunks(CHUNK_DAYS) {
        let start = chunk[0];
        let end = *chunk.last().expect("non-empty chunk");
        connection.execute("BEGIN TRANSACTION", Parameters::None)?;
        let result = (|| -> Result<usize> {
            connection.execute(
                format!(
                    "DELETE FROM {table} \
                     WHERE trade_date BETWEEN DATE '{start}' AND DATE '{end}' \
                       AND source_dataset = 'stock_moneyflow' \
                       AND calc_version = {}",
                    literal(calc_version)
                ),
                Parameters::None,
            )?;
            // The window function keeps the newest applicable membership when
            // a stock ever matches more than one interval on the same day.
            let sql = format!(
                r#"
INSERT INTO {table} (
  source_dataset, industry_code, industry_name, trade_date, stock_count,
  buy_sm_amount, sell_sm_amount, net_sm_amount,
  buy_md_amount, sell_md_amount, net_md_amount,
  buy_lg_amount, sell_lg_amount, net_lg_amount,
  buy_elg_amount, sell_elg_amount, net_elg_amount,
  main_net_amount, net_mf_vol, net_mf_amount, calc_version
)
WITH mapped AS (
  SELECT
    f.trade_date,
    m.l2_code,
    m.l2_name,
    f.buy_sm_amount, f.sell_sm_amount,
    f.buy_md_amount, f.sell_md_amount,
    f.buy_lg_amount, f.sell_lg_amount,
    f.buy_elg_amount, f.sell_elg_amount,
    f.net_mf_vol, f.net_mf_amount,
    row_number() OVER (
      PARTITION BY f.ts_code, f.trade_date
      ORDER BY m.in_date DESC
    ) AS rn
  FROM stock_moneyflow AS f
  JOIN index_sw_member AS m
    ON m.ts_code = f.ts_code
   AND f.trade_date >= m.in_date
   AND (m.out_date IS NULL OR f.trade_date < m.out_date)
  WHERE f.trade_date BETWEEN DATE '{start}' AND DATE '{end}'
    AND m.l2_code IS NOT NULL
)
SELECT
  'stock_moneyflow',
  l2_code,
  any_value(l2_name),
  trade_date,
  count(*),
  sum(buy_sm_amount), sum(sell_sm_amount), sum(buy_sm_amount - sell_sm_amount),
  sum(buy_md_amount), sum(sell_md_amount), sum(buy_md_amount - sell_md_amount),
  sum(buy_lg_amount), sum(sell_lg_amount), sum(buy_lg_amount - sell_lg_amount),
  sum(buy_elg_amount), sum(sell_elg_amount), sum(buy_elg_amount - sell_elg_amount),
  sum(buy_lg_amount - sell_lg_amount + buy_elg_amount - sell_elg_amount),
  sum(net_mf_vol),
  sum(net_mf_amount),
  {version}
FROM mapped
WHERE rn = 1
GROUP BY l2_code, trade_date
"#,
                version = literal(calc_version)
            );
            connection.execute(&sql, Parameters::None)?;
            scalar_i64(
                connection,
                &format!(
                    "SELECT count(*)::BIGINT FROM {table} \
                     WHERE trade_date BETWEEN DATE '{start}' AND DATE '{end}' \
                       AND calc_version = {}",
                    literal(calc_version)
                ),
            )
            .map(|count| usize::try_from(count).unwrap_or(0))
        })();
        match result {
            Ok(rows) => {
                connection.execute("COMMIT", Parameters::None)?;
                written += rows;
            }
            Err(error) => {
                let _ = connection.execute("ROLLBACK", Parameters::None);
                return Err(error);
            }
        }
    }
    if written == 0 {
        eprintln!(
            "[write] industry moneyflow: no stocks mapped to SW2021 L2 industries in {first}..{last}"
        );
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    fn setup(connection: &Connection) -> Result<()> {
        duckdb::create_schema(connection)?;
        // A: two active intervals; the 2020 interval must win.
        for (ts_code, l2_code, in_date) in [
            ("000001.SZ", "801014.SI", "2019-01-01"),
            ("000001.SZ", "801016.SI", "2020-01-01"),
            ("000002.SZ", "801016.SI", "2019-01-01"),
            ("000003.SZ", "801014.SI", "2020-01-01"),
            // Expired before the probe date.
            ("000004.SZ", "801014.SI", "2019-01-01"),
            // Active membership but no moneyflow row.
            ("000005.SZ", "801014.SI", "2020-01-01"),
        ] {
            let out_date = if ts_code == "000004.SZ" {
                "DATE '2020-01-02'"
            } else {
                "NULL"
            };
            connection.execute(
                format!(
                    "INSERT INTO index_sw_member (l1_code,l1_name,l2_code,l2_name,l3_code,l3_name,ts_code,name,in_date,out_date,is_new) \
                     VALUES ('801010.SI','农林牧渔','{l2_code}','行业{l2_code}','850000.SI','三级','{ts_code}','名称',DATE '{in_date}',{out_date},'Y')"
                ),
                Parameters::None,
            )?;
        }
        // 000006.SZ is not in any SW industry.
        for (ts_code, net_mf_amount) in [
            ("000001.SZ", 10.0),
            ("000002.SZ", 20.0),
            ("000003.SZ", 30.0),
            ("000004.SZ", 40.0),
            ("000006.SZ", 50.0),
        ] {
            connection.execute(
                format!(
                    "INSERT INTO stock_moneyflow (ts_code,trade_date,buy_sm_amount,sell_sm_amount,buy_md_amount,sell_md_amount,buy_lg_amount,sell_lg_amount,buy_elg_amount,sell_elg_amount,net_mf_vol,net_mf_amount) \
                     VALUES ('{ts_code}',DATE '2020-01-03',1,0,2,0,3,0,4,0,5,{net_mf_amount})"
                ),
                Parameters::None,
            )?;
        }
        Ok(())
    }

    fn values(connection: &Connection) -> Result<BTreeMap<String, (i64, f64)>> {
        let mut query = connection.query(
            "SELECT industry_code, stock_count, net_mf_amount FROM calc_daily_industry_moneyflow \
             WHERE trade_date = DATE '2020-01-03' ORDER BY industry_code",
            Parameters::None,
        )?;
        let mut rows = BTreeMap::new();
        while let Some(chunk) = query.next_chunk()? {
            let codes = chunk.get_vector_at::<String>(0)?.reader()?;
            let counts = chunk.get_vector_at::<i64>(1)?.reader()?;
            let amounts = chunk.get_vector_at::<f64>(2)?.reader()?;
            for index in 0..chunk.row_count()? {
                rows.insert(
                    codes.get(index)?.unwrap_or("").to_string(),
                    (
                        counts.get(index)?.copied().unwrap_or(0),
                        amounts.get(index)?.copied().unwrap_or(0.0),
                    ),
                );
            }
        }
        Ok(rows)
    }

    #[test]
    fn aggregates_mapped_stocks_once_and_respects_intervals() {
        duckdb::with_connection(":memory:", |connection| {
            setup(connection)?;
            let written = rebuild(connection, &[day("2020-01-03")], "test-v1")?;
            assert_eq!(written, 2);
            let rows = values(connection)?;
            assert_eq!(rows["801014.SI"], (1, 30.0));
            assert_eq!(rows["801016.SI"], (2, 30.0));
            // 000004.SZ expired, 000006.SZ unmapped.
            assert_eq!(rows.values().map(|(_, amount)| amount).sum::<f64>(), 60.0);
            // Rerunning the same window is idempotent.
            assert_eq!(rebuild(connection, &[day("2020-01-03")], "test-v1")?, 2);
            assert_eq!(
                duckdb::table_count(connection, duckdb::INDUSTRY_MONEYFLOW_TABLE)?,
                2
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn skips_without_source_rows_and_requires_membership() {
        duckdb::with_connection(":memory:", |connection| {
            duckdb::create_schema(connection)?;
            assert_eq!(rebuild(connection, &[day("2020-01-03")], "test-v1")?, 0);
            connection.execute(
                "INSERT INTO stock_moneyflow (ts_code,trade_date) VALUES ('000001.SZ',DATE '2020-01-03')",
                Parameters::None,
            )?;
            let error = rebuild(connection, &[day("2020-01-03")], "test-v1").unwrap_err();
            assert!(error.to_string().contains("index_sw_member is empty"));
            Ok(())
        })
        .unwrap();
    }
}
