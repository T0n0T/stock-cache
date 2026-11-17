#!/usr/bin/env python3
"""Generate docs/tushare-fields.md from schemas/tushare-fields.yaml."""

from __future__ import annotations

import argparse
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SCHEMA = REPO_ROOT / "schemas" / "tushare-fields.yaml"
DEFAULT_OUTPUT = REPO_ROOT / "docs" / "tushare-fields.md"


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


def fmt_list(value: Any) -> str:
    if isinstance(value, list):
        return ", ".join(f"`{item}`" for item in value)
    return f"`{value}`"


def fmt_strategy(strategy: dict[str, Any]) -> str:
    items = []
    for key, value in strategy.items():
        items.append(f"`{key}`={fmt_list(value)}")
    return "; ".join(items)


def fmt_fetch_plan(fetch_plan: dict[str, Any]) -> str:
    items = []
    for key, value in fetch_plan.items():
        items.append(f"`{key}`={fmt_list(value)}")
    return "; ".join(items)


def render_doc(schema: dict[str, Any]) -> str:
    lines: list[str] = []
    lines.append("# Tushare Fields")
    lines.append("")
    lines.append("This file is generated from `schemas/tushare-fields.yaml`.")
    lines.append("Do not edit it by hand; update the YAML and rerun `tools/generate_tushare_fields_doc.py`.")
    lines.append("")

    datasets = require_mapping(schema["datasets"], "datasets")
    for dataset_id, raw_dataset in datasets.items():
        dataset = require_mapping(raw_dataset, f"datasets.{dataset_id}")
        if dataset_id != dataset.get("table", dataset_id):
            raise SystemExit(f"dataset id must equal table name: {dataset_id}")

        fields = require_list(dataset.get("fields"), f"datasets.{dataset_id}.fields")
        lines.append(f"## {dataset_id}")
        lines.append("")
        lines.append(f"- API: `{dataset['api']}`")
        lines.append(f"- Official doc: {dataset['doc_url']}")
        lines.append(f"- Checked at: `{dataset['checked_at']}`")
        lines.append(f"- SDK version: `{dataset['sdk_version']}`")
        lines.append(f"- Fetch plan: {fmt_fetch_plan(require_mapping(dataset['fetch_plan'], dataset_id + '.fetch_plan'))}")
        lines.append(
            f"- Write strategy: {fmt_strategy(require_mapping(dataset['write_strategy'], dataset_id + '.write_strategy'))}"
        )
        for note in dataset.get("notes", []) or []:
            lines.append(f"- Note: {note}")
        for mismatch in dataset.get("doc_mismatch", []) or []:
            lines.append(f"- Doc mismatch: {mismatch}")
        lines.append("")
        lines.append("| Field | DuckDB type | Nullable | Source format | Description |")
        lines.append("| --- | --- | --- | --- | --- |")
        for raw_field in fields:
            field = require_mapping(raw_field, f"datasets.{dataset_id}.fields[]")
            required_keys = {"name", "duckdb_type", "nullable", "description"}
            missing = sorted(required_keys - field.keys())
            if missing:
                raise SystemExit(f"{dataset_id}.{field.get('name', '<unknown>')} missing keys: {missing}")
            source_format = field.get("source_format", "")
            nullable = "yes" if field["nullable"] else "no"
            lines.append(
                "| "
                f"`{field['name']}` | `{field['duckdb_type']}` | {nullable} | {source_format} | {field['description']} |"
            )
        lines.append("")
    return "\n".join(lines).rstrip() + "\n"


def main() -> int:
    args = parse_args()
    schema = load_yaml(args.schema)
    rendered = render_doc(schema)
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
