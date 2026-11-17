#!/usr/bin/env python3
"""Generate static Rust Tushare dataset metadata from schemas/tushare-fields.yaml."""

from __future__ import annotations

import argparse
import re
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SCHEMA = REPO_ROOT / "schemas" / "tushare-fields.yaml"
DEFAULT_OUTPUT = REPO_ROOT / "src" / "metadata" / "tushare_fields.rs"

FORBIDDEN_SOURCE_FIELDS = {
    "fetched_at",
    "ingest_run_id",
    "updated_at",
}


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


def require_mapping(value: Any, context: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise SystemExit(f"{context} must be a mapping")
    return value


def require_list(value: Any, context: str) -> list[Any]:
    if not isinstance(value, list):
        raise SystemExit(f"{context} must be a list")
    return value


def require_string(value: Any, context: str) -> str:
    if not isinstance(value, str):
        raise SystemExit(f"{context} must be a string")
    return value


def require_bool(value: Any, context: str) -> bool:
    if not isinstance(value, bool):
        raise SystemExit(f"{context} must be a boolean")
    return value


def require_string_list(value: Any, context: str) -> list[str]:
    return [require_string(item, f"{context}[]") for item in require_list(value, context)]


def rust_str(value: str) -> str:
    return '"' + value.replace('\\', '\\\\').replace('"', '\\"') + '"'


def rust_option_str(value: Any) -> str:
    if value is None:
        return "None"
    return f"Some({rust_str(require_string(value, 'option value'))})"


def rust_str_slice(values: list[str]) -> str:
    if not values:
        return "&[]"
    return "&[" + ", ".join(rust_str(value) for value in values) + "]"


def const_name(dataset_id: str, suffix: str) -> str:
    value = re.sub(r"[^a-zA-Z0-9]+", "_", dataset_id).upper().strip("_")
    return f"{value}_{suffix}"


def validate_dataset(dataset_id: str, dataset: dict[str, Any]) -> None:
    table = dataset.get("table", dataset_id)
    if table != dataset_id:
        raise SystemExit(f"dataset id must equal table name: {dataset_id}")
    require_string(dataset.get("api"), f"datasets.{dataset_id}.api")
    require_mapping(dataset.get("write_strategy"), f"datasets.{dataset_id}.write_strategy")
    require_mapping(dataset.get("fetch_plan"), f"datasets.{dataset_id}.fetch_plan")

    fields = require_list(dataset.get("fields"), f"datasets.{dataset_id}.fields")
    if not fields:
        raise SystemExit(f"datasets.{dataset_id}.fields must not be empty")
    for raw_field in fields:
        field = require_mapping(raw_field, f"datasets.{dataset_id}.fields[]")
        name = require_string(field.get("name"), f"datasets.{dataset_id}.fields[].name")
        if name in FORBIDDEN_SOURCE_FIELDS:
            raise SystemExit(f"{dataset_id}.{name} is forbidden local source-table metadata")
        duckdb_type = require_string(
            field.get("duckdb_type"), f"datasets.{dataset_id}.{name}.duckdb_type"
        )
        if duckdb_type not in {"DATE", "TIMESTAMP", "BIGINT", "DOUBLE", "VARCHAR"}:
            raise SystemExit(
                f"unsupported DuckDB type for {dataset_id}.{name}: {duckdb_type}"
            )
        require_bool(field.get("nullable"), f"datasets.{dataset_id}.{name}.nullable")
        if "source_format" in field:
            require_string(field["source_format"], f"datasets.{dataset_id}.{name}.source_format")


def render_field_array(dataset_id: str, dataset: dict[str, Any]) -> str:
    lines = [f"const {const_name(dataset_id, 'FIELDS')}: &[FieldMetadata] = &["]
    for raw_field in require_list(dataset["fields"], f"datasets.{dataset_id}.fields"):
        field = require_mapping(raw_field, f"datasets.{dataset_id}.fields[]")
        source_format = rust_option_str(field.get("source_format"))
        lines.extend(
            [
                "    FieldMetadata {",
                f"        name: {rust_str(field['name'])},",
                f"        duckdb_type: {rust_str(field['duckdb_type'])},",
                f"        nullable: {str(field['nullable']).lower()},",
                f"        source_format: {source_format},",
                "    },",
            ]
        )
    lines.append("];\n")
    return "\n".join(lines)


def render_write_strategy(dataset_id: str, raw_strategy: Any) -> str:
    strategy = require_mapping(raw_strategy, f"datasets.{dataset_id}.write_strategy")
    strategy_type = require_string(strategy.get("type"), f"datasets.{dataset_id}.write_strategy.type")
    if strategy_type == "upsert":
        key = require_string_list(strategy.get("key"), f"datasets.{dataset_id}.write_strategy.key")
        return f"WriteStrategy::Upsert {{\n            key: {rust_str_slice(key)},\n        }}"
    if strategy_type == "replace_all":
        key = require_string_list(strategy.get("key"), f"datasets.{dataset_id}.write_strategy.key")
        transaction_group = rust_option_str(strategy.get("transaction_group"))
        return (
            "WriteStrategy::ReplaceAll {\n"
            f"            key: {rust_str_slice(key)},\n"
            f"            transaction_group: {transaction_group},\n"
            "        "
            "}"
        )
    if strategy_type == "delete_insert_by_date":
        date_field = require_string(
            strategy.get("date_field"), f"datasets.{dataset_id}.write_strategy.date_field"
        )
        natural_key = require_string_list(
            strategy.get("natural_key_candidate", []),
            f"datasets.{dataset_id}.write_strategy.natural_key_candidate",
        )
        return (
            "WriteStrategy::DeleteInsertByDate {\n"
            f"            date_field: {rust_str(date_field)},\n"
            f"            natural_key_candidate: {rust_str_slice(natural_key)},\n"
            "        "
            "}"
        )
    raise SystemExit(f"unsupported write strategy for {dataset_id}: {strategy_type}")


def render_fetch_plan(dataset_id: str, raw_fetch_plan: Any) -> str:
    fetch_plan = require_mapping(raw_fetch_plan, f"datasets.{dataset_id}.fetch_plan")
    kind = require_string(fetch_plan.get("kind"), f"datasets.{dataset_id}.fetch_plan.kind")
    params = require_string_list(fetch_plan.get("params"), f"datasets.{dataset_id}.fetch_plan.params")
    code_source = rust_option_str(fetch_plan.get("code_source"))
    return (
        "FetchPlan {\n"
        f"            kind: {rust_str(kind)},\n"
        f"            params: {rust_str_slice(params)},\n"
        f"            code_source: {code_source},\n"
        "        }"
    )


def render_dataset(dataset_id: str, dataset: dict[str, Any]) -> str:
    fields_const = const_name(dataset_id, "FIELDS")
    write_strategy = render_write_strategy(dataset_id, dataset["write_strategy"])
    fetch_plan = render_fetch_plan(dataset_id, dataset["fetch_plan"])
    return "\n".join(
        [
            "    DatasetMetadata {",
            f"        id: {rust_str(dataset_id)},",
            f"        table: {rust_str(dataset_id)},",
            f"        api: {rust_str(dataset['api'])},",
            f"        fields: {fields_const},",
            f"        write_strategy: {write_strategy},",
            f"        fetch_plan: {fetch_plan},",
            "    },",
        ]
    )


def render(schema: dict[str, Any]) -> str:
    datasets = require_mapping(schema["datasets"], "datasets")
    for dataset_id, raw_dataset in datasets.items():
        validate_dataset(str(dataset_id), require_mapping(raw_dataset, f"datasets.{dataset_id}"))

    lines = [
        "// Generated from schemas/tushare-fields.yaml by tools/generate_rust_dataset_metadata.py.",
        "// Do not edit by hand.",
        "#![allow(dead_code)]",
        "",
        "#[derive(Debug, Clone, Copy, Eq, PartialEq)]",
        "pub struct DatasetMetadata {",
        "    pub id: &'static str,",
        "    pub table: &'static str,",
        "    pub api: &'static str,",
        "    pub fields: &'static [FieldMetadata],",
        "    pub write_strategy: WriteStrategy,",
        "    pub fetch_plan: FetchPlan,",
        "}",
        "",
        "#[derive(Debug, Clone, Copy, Eq, PartialEq)]",
        "pub struct FieldMetadata {",
        "    pub name: &'static str,",
        "    pub duckdb_type: &'static str,",
        "    pub nullable: bool,",
        "    pub source_format: Option<&'static str>,",
        "}",
        "",
        "#[derive(Debug, Clone, Copy, Eq, PartialEq)]",
        "pub enum WriteStrategy {",
        "    Upsert {",
        "        key: &'static [&'static str],",
        "    },",
        "    ReplaceAll {",
        "        key: &'static [&'static str],",
        "        transaction_group: Option<&'static str>,",
        "    },",
        "    DeleteInsertByDate {",
        "        date_field: &'static str,",
        "        natural_key_candidate: &'static [&'static str],",
        "    },",
        "}",
        "",
        "#[derive(Debug, Clone, Copy, Eq, PartialEq)]",
        "pub struct FetchPlan {",
        "    pub kind: &'static str,",
        "    pub params: &'static [&'static str],",
        "    pub code_source: Option<&'static str>,",
        "}",
        "",
        "impl DatasetMetadata {",
        "    pub fn field(&self, name: &str) -> Option<&FieldMetadata> {",
        "        self.fields.iter().find(|field| field.name == name)",
        "    }",
        "}",
        "",
        "pub fn dataset(id: &str) -> Option<&'static DatasetMetadata> {",
        "    DATASETS.iter().find(|dataset| dataset.id == id)",
        "}",
        "",
    ]

    for dataset_id, raw_dataset in datasets.items():
        dataset = require_mapping(raw_dataset, f"datasets.{dataset_id}")
        lines.append(render_field_array(str(dataset_id), dataset))

    lines.append("pub const DATASETS: &[DatasetMetadata] = &[")
    for dataset_id, raw_dataset in datasets.items():
        dataset = require_mapping(raw_dataset, f"datasets.{dataset_id}")
        lines.append(render_dataset(str(dataset_id), dataset))
    lines.append("];\n")
    return "\n".join(lines).rstrip() + "\n"


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
