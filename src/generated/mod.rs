pub mod tushare_fields;

#[cfg(test)]
mod tests {
    use super::tushare_fields::{dataset, WriteSql, WriteStrategy, DATASETS};

    fn field_names(dataset_id: &str) -> Vec<&'static str> {
        dataset(dataset_id)
            .expect("dataset exists")
            .fields
            .iter()
            .map(|field| field.name)
            .collect()
    }

    #[test]
    fn generated_metadata_contains_nine_datasets() {
        let expected = [
            ("stock_stk_factor_pro", "stk_factor_pro", 261),
            ("stock_moneyflow_ths", "moneyflow_ths", 13),
            ("index_idx_factor_pro", "idx_factor_pro", 89),
            ("index_ths", "ths_index", 6),
            ("index_ths_daily", "ths_daily", 14),
            ("index_moneyflow_cnt_ths", "moneyflow_cnt_ths", 12),
            ("index_ths_member", "ths_member", 7),
            ("index_limit_cpt_list", "limit_cpt_list", 9),
            ("stock_limit_list_ths", "limit_list_ths", 24),
        ];

        assert_eq!(DATASETS.len(), 9);
        for (dataset, (expected_id, expected_api, expected_field_count)) in
            DATASETS.iter().zip(expected)
        {
            assert_eq!(dataset.id, expected_id);
            assert_eq!(dataset.table, expected_id);
            assert_eq!(dataset.api, expected_api);
            assert_eq!(dataset.fields.len(), expected_field_count);
        }
        assert_eq!(
            DATASETS
                .iter()
                .map(|dataset| dataset.fields.len())
                .sum::<usize>(),
            435
        );
    }

    #[test]
    fn index_ths_daily_keeps_optional_market_value_fields() {
        let fields = field_names("index_ths_daily");

        assert!(fields.contains(&"total_mv"));
        assert!(fields.contains(&"float_mv"));
    }

    #[test]
    fn index_ths_daily_fetch_plan_uses_whole_day_python_function_shape() {
        let index_ths_daily = dataset("index_ths_daily").expect("index_ths_daily exists");

        assert_eq!(index_ths_daily.fetch_plan.kind, "by_trade_date");
        assert_eq!(index_ths_daily.fetch_plan.params, &["trade_date"]);
        assert_eq!(index_ths_daily.fetch_plan.code_source, None);
    }

    #[test]
    fn index_ths_member_keeps_documented_optional_membership_fields() {
        let fields = field_names("index_ths_member");

        for required in ["weight", "in_date", "out_date", "is_new"] {
            assert!(fields.contains(&required), "missing {required}");
        }
    }

    #[test]
    fn stock_limit_list_ths_keeps_all_fields_and_timestamp_formats() {
        let limit_list = dataset("stock_limit_list_ths").expect("stock_limit_list_ths exists");

        assert_eq!(limit_list.fields.len(), 24);
        for name in [
            "first_lu_time",
            "last_lu_time",
            "first_ld_time",
            "last_ld_time",
        ] {
            let field = limit_list.field(name).expect("timestamp field exists");
            assert_eq!(field.pg_type, "timestamp without time zone");
            assert_eq!(field.source_format, Some("YYYY-MM-DD HH:MM:SS"));
        }
    }

    #[test]
    fn index_limit_cpt_list_uses_exact_trade_date_delete_insert_strategy() {
        let limit_cpt = dataset("index_limit_cpt_list").expect("index_limit_cpt_list exists");

        assert_eq!(
            limit_cpt.write_strategy,
            WriteStrategy::DeleteInsertByDate {
                date_field: "trade_date",
                natural_key_candidate: &["trade_date", "rank", "ts_code"],
            }
        );
    }

    #[test]
    fn source_metadata_has_no_local_or_json_escape_hatch_fields() {
        let forbidden = [
            "raw_jsonb",
            "extra_jsonb",
            "fetched_at",
            "ingest_run_id",
            "updated_at",
        ];

        for dataset in DATASETS {
            for field in dataset.fields {
                assert!(
                    !forbidden.contains(&field.name),
                    "{} must not contain forbidden field {}",
                    dataset.id,
                    field.name
                );
            }
        }
    }

    #[test]
    fn generated_write_sql_kind_matches_write_strategy() {
        for dataset in DATASETS {
            assert!(
                matches!(
                    (dataset.write_strategy, dataset.write_sql),
                    (WriteStrategy::Upsert { .. }, WriteSql::Upsert { .. })
                        | (
                            WriteStrategy::ReplaceAll { .. },
                            WriteSql::ReplaceAll { .. }
                        )
                        | (
                            WriteStrategy::DeleteInsertByDate { .. },
                            WriteSql::DeleteInsertByDate { .. }
                        )
                ),
                "{} has mismatched write strategy and SQL kind",
                dataset.id
            );
        }
    }
}
