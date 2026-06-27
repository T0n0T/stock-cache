#!/usr/bin/env python3
"""Generate static PostgreSQL schema SQL from schemas/tushare-fields.yaml."""

from __future__ import annotations

import argparse
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SCHEMA = REPO_ROOT / "schemas" / "tushare-fields.yaml"
DEFAULT_OUTPUT = REPO_ROOT / "deploy" / "postgres" / "schema.sql"


CALC_TABLES_SQL = """
create table "stock_period_indicator_state" (
    "ts_code" text not null,
    "period_type" text not null,
    "period_start_date" date,
    "period_end_date" date not null,
    "period_close_qfq" double precision,
    "ema12" double precision,
    "ema26" double precision,
    "dif" double precision,
    "dea" double precision,
    "hist_x2" double precision,
    "period_count" integer,
    "calc_version" text not null,
    check ("period_type" in ('daily', 'weekly', 'monthly')),
    primary key ("ts_code", "period_type", "period_end_date", "calc_version")
);

create index "stock_period_indicator_state_lookup_idx"
    on "stock_period_indicator_state" ("ts_code", "period_type", "period_end_date" desc);

create table "stock_daily_asof_indicators" (
    "ts_code" text not null,
    "trade_date" date not null,
    "daily_dif_asof" double precision,
    "daily_dea_asof" double precision,
    "daily_hist_x2_asof" double precision,
    "daily_dea_pctile_asof" double precision,
    "daily_period_count" integer,
    "weekly_dif_asof" double precision,
    "weekly_dea_asof" double precision,
    "weekly_hist_x2_asof" double precision,
    "weekly_dea_pctile_asof" double precision,
    "weekly_period_count" integer,
    "monthly_dif_asof" double precision,
    "monthly_dea_asof" double precision,
    "monthly_hist_x2_asof" double precision,
    "monthly_dea_pctile_asof" double precision,
    "monthly_period_count" integer,
    "calc_version" text not null,
    primary key ("ts_code", "trade_date", "calc_version")
);

create table "index_daily_asof_indicators" (
    "ts_code" text not null,
    "trade_date" date not null,
    "daily_dif_asof" double precision,
    "daily_dea_asof" double precision,
    "daily_hist_x2_asof" double precision,
    "daily_dea_pctile_asof" double precision,
    "daily_period_count" integer,
    "weekly_dif_asof" double precision,
    "weekly_dea_asof" double precision,
    "weekly_hist_x2_asof" double precision,
    "weekly_dea_pctile_asof" double precision,
    "weekly_period_count" integer,
    "monthly_dif_asof" double precision,
    "monthly_dea_asof" double precision,
    "monthly_hist_x2_asof" double precision,
    "monthly_dea_pctile_asof" double precision,
    "monthly_period_count" integer,
    "calc_version" text not null,
    primary key ("ts_code", "trade_date", "calc_version")
);

create table "stock_daily_rolling_factors" (
    "ts_code" text not null,
    "trade_date" date not null,
    "ma25_qfq" double precision,
    "ma144_qfq" double precision,
    "ma220_qfq" double precision,
    "high_20_qfq" double precision,
    "high_90_qfq" double precision,
    "low_90_qfq" double precision,
    "high_120_qfq" double precision,
    "low_120_qfq" double precision,
    "close_to_20d_max_close_pct" double precision,
    "position_90d" double precision,
    "position_120d" double precision,
    "volume_ma5" double precision,
    "volume_ma20" double precision,
    "volume_to_ma5_ratio" double precision,
    "volume_to_ma20_ratio" double precision,
    "volume_ma5_to_ma20_ratio" double precision,
    "turnover_rate_ma5" double precision,
    "turnover_to_ma5_ratio" double precision,
    "range_compression_20d" double precision,
    "range_compression_40d" double precision,
    "calc_version" text not null,
    primary key ("ts_code", "trade_date", "calc_version")
);

create table "stock_daily_left_peak" (
    "ts_code" text not null,
    "trade_date" date not null,
    "left_peak_date" date,
    "left_peak_high" double precision,
    "breakout_date" date,
    "breakout_close" double precision,
    "breakout_body_above_left_peak" boolean,
    "first_bear_date" date,
    "first_bear_open" double precision,
    "first_bear_missing" boolean,
    "pick_close" double precision,
    "b_div_a" double precision,
    "abs_ba_minus_1" double precision,
    "a_lt_b" boolean,
    "is_valid" boolean,
    "status" text not null,
    "calc_version" text not null,
    check ("status" in (
        'insufficient_history',
        'no_swing_high',
        'no_cycle_peak',
        'no_close_breakout',
        'no_body_breakout',
        'pullback_too_small',
        'valid'
    )),
    primary key ("ts_code", "trade_date", "calc_version")
);
""".strip()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--schema", type=Path, default=DEFAULT_SCHEMA)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--check", action="store_true", help="Fail if output would change.")
    return parser.parse_args()


def load_yaml(path: Path) -> dict[str, Any]:
    try:
        import yaml  # type: ignore
    except ImportError as exc:
        raise SystemExit("Missing PyYAML. Install with: python -m pip install pyyaml") from exc

    with path.open("r", encoding="utf-8") as handle:
        data = yaml.safe_load(handle)
    if not isinstance(data, dict) or not isinstance(data.get("datasets"), dict):
        raise SystemExit(f"{path} must contain a top-level datasets mapping")
    return data


def quote_ident(value: str) -> str:
    return '"' + value.replace('"', '""') + '"'


def column_sql(field: dict[str, Any], required_key: bool) -> str:
    nullable = bool(field["nullable"])
    not_null = " not null" if required_key or not nullable else ""
    return f"    {quote_ident(field['name'])} {field['pg_type']}{not_null}"


def primary_key_for(dataset: dict[str, Any]) -> list[str]:
    strategy = dataset.get("write_strategy") or {}
    if strategy.get("type") == "delete_insert_by_date":
        return []
    key = strategy.get("key") or []
    if not isinstance(key, list):
        return []
    return [str(item) for item in key]


def render_source_table(dataset_id: str, dataset: dict[str, Any]) -> str:
    fields = dataset.get("fields") or []
    if not isinstance(fields, list) or not fields:
        raise SystemExit(f"{dataset_id} must have fields")

    primary_key = primary_key_for(dataset)
    primary_key_set = set(primary_key)
    lines = [f"create table {quote_ident(dataset_id)} ("]
    column_lines = [column_sql(field, str(field["name"]) in primary_key_set) for field in fields]
    if primary_key:
        pk_columns = ", ".join(quote_ident(column) for column in primary_key)
        column_lines.append(f"    primary key ({pk_columns})")
    lines.append(",\n".join(column_lines))
    lines.append(");")

    index_lines: list[str] = []
    if dataset_id == "index_ths_member":
        index_lines.extend(
            [
                'create index "index_ths_member_con_code_idx" on "index_ths_member" ("con_code");',
                'create index "index_ths_member_ts_code_idx" on "index_ths_member" ("ts_code");',
            ]
        )
    if dataset.get("write_strategy", {}).get("type") == "delete_insert_by_date":
        date_field = dataset["write_strategy"].get("date_field")
        if date_field:
            index_lines.append(
                f"create index {quote_ident(dataset_id + '_' + date_field + '_idx')} "
                f"on {quote_ident(dataset_id)} ({quote_ident(str(date_field))});"
            )
    if index_lines:
        lines.append("")
        lines.extend(index_lines)
    return "\n".join(lines)


def render(schema: dict[str, Any]) -> str:
    datasets = schema["datasets"]
    sections = [
        "-- Generated from schemas/tushare-fields.yaml by tools/generate_schema_sql.py.",
        "-- Do not edit by hand.",
        "",
    ]
    for dataset_id, dataset in datasets.items():
        sections.append(render_source_table(str(dataset_id), dataset))
        sections.append("")
    sections.append(CALC_TABLES_SQL)
    return "\n".join(sections).rstrip() + "\n"


def main() -> int:
    args = parse_args()
    rendered = render(load_yaml(args.schema))
    if args.check:
        existing = args.output.read_text(encoding="utf-8") if args.output.exists() else None
        if existing != rendered:
            raise SystemExit(f"{args.output} is out of date")
        return 0
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(rendered, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
