use anyhow::{anyhow, Result};
use chrono::NaiveDate;

use crate::{app::write::DateWindow, source::writer::parse_date_text};

use super::model::SourceBar;

pub(super) async fn resolve_calc_dates(
    client: &tokio_postgres::Client,
    window: &DateWindow,
) -> Result<Vec<NaiveDate>> {
    match window {
        DateWindow::Explicit {
            start_date,
            end_date,
        } => {
            let start = parse_date_text(start_date)?;
            let end = parse_date_text(end_date)?;
            let rows = client
                .query(
                    r#"
                    select distinct "trade_date"
                    from "stock_stk_factor_pro"
                    where "trade_date" between $1 and $2
                    order by "trade_date"
                    "#,
                    &[&start, &end],
                )
                .await?;
            Ok(rows.into_iter().map(|row| row.get(0)).collect())
        }
        DateWindow::LookbackTradingDays(days) | DateWindow::DefaultLookbackTradingDays(days) => {
            let limit = i64::from(*days);
            let rows = client
                .query(
                    r#"
                    select distinct "trade_date"
                    from "stock_stk_factor_pro"
                    order by "trade_date" desc
                    limit $1
                    "#,
                    &[&limit],
                )
                .await?;
            let mut dates = rows.into_iter().map(|row| row.get(0)).collect::<Vec<_>>();
            dates.reverse();
            Ok(dates)
        }
        DateWindow::RefreshOnly => Err(anyhow!(
            "calc execution requires a stock_stk_factor_pro date window"
        )),
    }
}

pub(super) async fn load_source_rows(
    client: &tokio_postgres::Client,
    max_trade_date: NaiveDate,
) -> Result<Vec<SourceBar>> {
    let rows = client
        .query(
            r#"
            select "ts_code", "trade_date", "open_qfq", "high_qfq", "low_qfq", "close_qfq", "vol", "turnover_rate"
            from "stock_stk_factor_pro"
            where "trade_date" <= $1
            order by "ts_code", "trade_date"
            "#,
            &[&max_trade_date],
        )
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| SourceBar {
            ts_code: row.get("ts_code"),
            trade_date: row.get("trade_date"),
            open_qfq: row.get("open_qfq"),
            high_qfq: row.get("high_qfq"),
            low_qfq: row.get("low_qfq"),
            close_qfq: row.get("close_qfq"),
            vol: row.get("vol"),
            turnover_rate: row.get("turnover_rate"),
        })
        .collect())
}

pub(super) async fn load_index_source_rows(
    client: &tokio_postgres::Client,
    max_trade_date: NaiveDate,
) -> Result<Vec<SourceBar>> {
    let rows = client
        .query(
            r#"
            select "ts_code", "trade_date", "open", "high", "low", "close", "vol", "turnover_rate"
            from (
                select "ts_code", "trade_date", "open", "high", "low", "close", "vol", "turnover_rate"
                from "index_ths_daily"
                where "trade_date" <= $1
                union all
                select "ts_code", "trade_date", "open", "high", "low", "close", "vol", null::double precision as "turnover_rate"
                from "index_idx_factor_pro"
                where "trade_date" <= $1
            ) index_source
            order by "ts_code", "trade_date"
            "#,
            &[&max_trade_date],
        )
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| SourceBar {
            ts_code: row.get("ts_code"),
            trade_date: row.get("trade_date"),
            open_qfq: row.get("open"),
            high_qfq: row.get("high"),
            low_qfq: row.get("low"),
            close_qfq: row.get("close"),
            vol: row.get("vol"),
            turnover_rate: row.get("turnover_rate"),
        })
        .collect())
}

pub(super) async fn load_source_row_count(
    client: &tokio_postgres::Client,
    max_trade_date: NaiveDate,
) -> Result<usize> {
    let row = client
        .query_one(
            r#"
            select count(*)::bigint
            from "stock_stk_factor_pro"
            where "trade_date" <= $1
            "#,
            &[&max_trade_date],
        )
        .await?;
    let count: i64 = row.get(0);
    Ok(count as usize)
}

pub(super) async fn load_index_source_row_count(
    client: &tokio_postgres::Client,
    max_trade_date: NaiveDate,
) -> Result<usize> {
    let row = client
        .query_one(
            r#"
            select count(*)::bigint
            from (
                select "ts_code", "trade_date"
                from "index_ths_daily"
                where "trade_date" <= $1
                union all
                select "ts_code", "trade_date"
                from "index_idx_factor_pro"
                where "trade_date" <= $1
            ) index_source
            "#,
            &[&max_trade_date],
        )
        .await?;
    let count: i64 = row.get(0);
    Ok(count as usize)
}

pub(super) async fn load_distinct_source_trade_dates(
    client: &tokio_postgres::Client,
    max_trade_date: NaiveDate,
) -> Result<Vec<NaiveDate>> {
    let rows = client
        .query(
            r#"
            select distinct "trade_date"
            from "stock_stk_factor_pro"
            where "trade_date" <= $1
            order by "trade_date"
            "#,
            &[&max_trade_date],
        )
        .await?;
    Ok(rows.into_iter().map(|row| row.get(0)).collect())
}

pub(super) async fn load_distinct_index_source_trade_dates(
    client: &tokio_postgres::Client,
    max_trade_date: NaiveDate,
) -> Result<Vec<NaiveDate>> {
    let rows = client
        .query(
            r#"
            select distinct "trade_date"
            from (
                select "trade_date"
                from "index_ths_daily"
                where "trade_date" <= $1
                union all
                select "trade_date"
                from "index_idx_factor_pro"
                where "trade_date" <= $1
            ) index_source
            order by "trade_date"
            "#,
            &[&max_trade_date],
        )
        .await?;
    Ok(rows.into_iter().map(|row| row.get(0)).collect())
}
