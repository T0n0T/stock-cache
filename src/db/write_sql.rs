#![allow(dead_code)]

use crate::metadata::tushare_fields::{DatasetMetadata, WriteSql, WriteStrategy};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum WriteSqlPlan {
    Upsert {
        sql: &'static str,
    },
    ReplaceAll {
        clear_sql: &'static str,
        insert_sql: &'static str,
        transaction_group: Option<&'static str>,
    },
    DeleteInsertByDate {
        delete_sql: &'static str,
        insert_sql: &'static str,
        date_field: &'static str,
    },
}

pub fn static_write_sql(dataset: &DatasetMetadata) -> WriteSqlPlan {
    match (dataset.write_strategy, dataset.write_sql) {
        (WriteStrategy::Upsert { .. }, WriteSql::Upsert { sql }) => WriteSqlPlan::Upsert { sql },
        (
            WriteStrategy::ReplaceAll {
                transaction_group, ..
            },
            WriteSql::ReplaceAll {
                clear_sql,
                insert_sql,
            },
        ) => WriteSqlPlan::ReplaceAll {
            clear_sql,
            insert_sql,
            transaction_group,
        },
        (
            WriteStrategy::DeleteInsertByDate { date_field, .. },
            WriteSql::DeleteInsertByDate {
                delete_sql,
                insert_sql,
            },
        ) => WriteSqlPlan::DeleteInsertByDate {
            delete_sql,
            insert_sql,
            date_field,
        },
        _ => panic!(
            "generated write SQL kind does not match write strategy for {}",
            dataset.id
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::tushare_fields::{dataset, DATASETS};

    #[test]
    fn index_ths_daily_upsert_sql_is_quoted_and_contiguous() {
        let index_ths_daily = dataset("index_ths_daily").expect("dataset exists");
        let WriteSqlPlan::Upsert { sql } = static_write_sql(index_ths_daily) else {
            panic!("expected upsert plan");
        };

        assert!(sql
            .starts_with("insert into \"index_ths_daily\" (\"ts_code\", \"trade_date\", \"open\""));
        assert!(
            sql.contains("values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)")
        );
        assert!(sql.contains("on conflict (\"trade_date\", \"ts_code\") do update set"));
        assert!(sql.contains("\"change\" = excluded.\"change\""));
        assert!(!sql.contains("\"trade_date\" = excluded.\"trade_date\""));
        assert!(!sql.contains("\"ts_code\" = excluded.\"ts_code\""));
    }

    #[test]
    fn index_ths_replace_all_sql_quotes_reserved_columns() {
        let index_ths = dataset("index_ths").expect("dataset exists");
        let WriteSqlPlan::ReplaceAll {
            clear_sql,
            insert_sql,
            transaction_group,
        } = static_write_sql(index_ths)
        else {
            panic!("expected replace_all plan");
        };

        assert_eq!(clear_sql, "delete from \"index_ths\"");
        assert_eq!(transaction_group, Some("ths_classification"));
        assert_eq!(
            insert_sql,
            "insert into \"index_ths\" (\"ts_code\", \"name\", \"count\", \"exchange\", \"list_date\", \"type\") values ($1, $2, $3, $4, $5, $6)"
        );
        assert!(insert_sql.contains("\"count\""));
        assert!(insert_sql.contains("\"type\""));
    }

    #[test]
    fn index_ths_member_replace_all_sql_keeps_optional_fields() {
        let index_ths_member = dataset("index_ths_member").expect("dataset exists");
        let WriteSqlPlan::ReplaceAll {
            insert_sql,
            transaction_group,
            ..
        } = static_write_sql(index_ths_member)
        else {
            panic!("expected replace_all plan");
        };

        assert_eq!(transaction_group, Some("ths_classification"));
        for field in ["weight", "in_date", "out_date", "is_new"] {
            assert!(insert_sql.contains(&format!("\"{field}\"")));
        }
        assert!(insert_sql.contains("values ($1, $2, $3, $4, $5, $6, $7)"));
    }

    #[test]
    fn index_limit_cpt_list_delete_sql_uses_exact_trade_date_boundary() {
        let limit_cpt = dataset("index_limit_cpt_list").expect("dataset exists");
        let WriteSqlPlan::DeleteInsertByDate {
            delete_sql,
            date_field,
            ..
        } = static_write_sql(limit_cpt)
        else {
            panic!("expected delete_insert_by_date plan");
        };

        assert_eq!(date_field, "trade_date");
        assert_eq!(
            delete_sql,
            "delete from \"index_limit_cpt_list\" where \"trade_date\" = $1"
        );
        assert!(!delete_sql.contains(">="));
        assert!(!delete_sql.contains("<="));
        assert!(!delete_sql.to_lowercase().contains("between"));
        assert!(!delete_sql.contains("rank"));
        assert!(!delete_sql.contains("ts_code"));
    }

    #[test]
    fn index_limit_cpt_list_insert_sql_uses_all_declared_fields() {
        let limit_cpt = dataset("index_limit_cpt_list").expect("dataset exists");
        let WriteSqlPlan::DeleteInsertByDate { insert_sql, .. } = static_write_sql(limit_cpt)
        else {
            panic!("expected delete_insert_by_date plan");
        };

        assert_eq!(
            insert_sql,
            "insert into \"index_limit_cpt_list\" (\"ts_code\", \"name\", \"trade_date\", \"days\", \"up_stat\", \"cons_nums\", \"up_nums\", \"pct_chg\", \"rank\") values ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
        );
    }

    #[test]
    fn stock_limit_list_ths_upsert_sql_includes_timestamp_fields() {
        let limit_list = dataset("stock_limit_list_ths").expect("dataset exists");
        let WriteSqlPlan::Upsert { sql } = static_write_sql(limit_list) else {
            panic!("expected upsert plan");
        };

        for field in [
            "first_lu_time",
            "last_lu_time",
            "first_ld_time",
            "last_ld_time",
        ] {
            assert!(sql.contains(&format!("\"{field}\"")));
        }
        assert!(sql.contains("$24"));
        assert!(sql.contains("on conflict (\"trade_date\", \"ts_code\") do update set"));
    }

    #[test]
    fn static_sql_contains_no_forbidden_local_metadata_columns() {
        let forbidden = [
            "raw_jsonb",
            "extra_jsonb",
            "fetched_at",
            "ingest_run_id",
            "updated_at",
        ];

        for dataset in DATASETS {
            let rendered = format!("{:?}", static_write_sql(dataset));
            for field in forbidden {
                assert!(!rendered.contains(field), "{} contains {field}", dataset.id);
            }
        }
    }
}
