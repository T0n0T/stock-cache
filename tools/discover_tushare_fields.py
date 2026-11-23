#!/usr/bin/env python3
"""Discover Tushare SDK return fields for stock-cache source datasets.

This is a development helper only. It is not used by the Rust CLI runtime and
is a development-time field inspection utility.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from collections.abc import Callable
from pathlib import Path
from typing import Any


DEFAULT_STOCK_CODE = "000001.SZ"
DEFAULT_INDEX_CODE = "000001.SH"
REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SCHEMA = REPO_ROOT / "schemas" / "tushare-fields.yaml"


class DatasetSpec:
    def __init__(
        self,
        api: str,
        build_params: Callable[[argparse.Namespace, Any], dict[str, Any]],
    ) -> None:
        self.api = api
        self.build_params = build_params


def _date_params(args: argparse.Namespace, _pro: Any) -> dict[str, Any]:
    if args.start_date and args.end_date:
        return {"start_date": args.start_date, "end_date": args.end_date}
    return {"trade_date": args.trade_date}


def _stock_window_params(args: argparse.Namespace, _pro: Any) -> dict[str, Any]:
    params: dict[str, Any] = {"ts_code": args.stock_code}
    if args.start_date and args.end_date:
        params.update({"start_date": args.start_date, "end_date": args.end_date})
    else:
        params["trade_date"] = args.trade_date
    return params


def _index_window_params(args: argparse.Namespace, _pro: Any) -> dict[str, Any]:
    params: dict[str, Any] = {"ts_code": args.index_code}
    if args.start_date and args.end_date:
        params.update({"start_date": args.start_date, "end_date": args.end_date})
    else:
        params["trade_date"] = args.trade_date
    return params


def _index_weight_params(args: argparse.Namespace, _pro: Any) -> dict[str, Any]:
    params: dict[str, Any] = {"index_code": args.index_code}
    if args.start_date and args.end_date:
        params.update({"start_date": args.start_date, "end_date": args.end_date})
    else:
        params["trade_date"] = args.trade_date
    return params


def _ths_index_params(_args: argparse.Namespace, _pro: Any) -> dict[str, Any]:
    return {}


def _resolve_ths_code(args: argparse.Namespace, pro: Any) -> str:
    if args.ths_code:
        return args.ths_code
    df = pro.ths_index()
    if df.empty or "ts_code" not in df.columns:
        raise RuntimeError("ths_index returned no ts_code values; pass --ths-code explicitly")
    return str(df.iloc[0]["ts_code"])


def _ths_window_params(args: argparse.Namespace, pro: Any) -> dict[str, Any]:
    params: dict[str, Any] = {"ts_code": _resolve_ths_code(args, pro)}
    if args.start_date and args.end_date:
        params.update({"start_date": args.start_date, "end_date": args.end_date})
    else:
        params["trade_date"] = args.trade_date
    return params


def _ths_member_params(args: argparse.Namespace, pro: Any) -> dict[str, Any]:
    return {"ts_code": _resolve_ths_code(args, pro)}


DATASETS: dict[str, DatasetSpec] = {
    "stock_stk_factor_pro": DatasetSpec("stk_factor_pro", _stock_window_params),
    "index_idx_factor_pro": DatasetSpec("idx_factor_pro", _index_window_params),
    "index_weight": DatasetSpec("index_weight", _index_weight_params),
    "index_ths": DatasetSpec("ths_index", _ths_index_params),
    "index_ths_daily": DatasetSpec("ths_daily", _ths_window_params),
    "index_ths_member": DatasetSpec("ths_member", _ths_member_params),
    "index_limit_cpt_list": DatasetSpec("limit_cpt_list", _date_params),
    "stock_limit_list_ths": DatasetSpec("limit_list_ths", _date_params),
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Print SDK-returned fields, pandas dtype, non-null count, and sample values."
    )
    parser.add_argument(
        "datasets",
        nargs="*",
        choices=sorted(DATASETS),
        help="Dataset ids to inspect. Defaults to all supported datasets.",
    )
    parser.add_argument("--trade-date", default="20260625", help="YYYYMMDD date for daily APIs.")
    parser.add_argument("--start-date", help="YYYYMMDD start date for code-window APIs.")
    parser.add_argument("--end-date", help="YYYYMMDD end date for code-window APIs.")
    parser.add_argument("--stock-code", default=DEFAULT_STOCK_CODE, help="Stock ts_code sample.")
    parser.add_argument("--index-code", default=DEFAULT_INDEX_CODE, help="Index ts_code sample.")
    parser.add_argument("--ths-code", help="THS index code sample. Defaults to first index_ths row.")
    parser.add_argument("--max-samples", type=int, default=3, help="Sample values per field.")
    parser.add_argument("--format", choices=("json", "markdown"), default="json")
    parser.add_argument(
        "--schema",
        type=Path,
        default=DEFAULT_SCHEMA,
        help="YAML schema to read explicit fields from.",
    )
    parser.add_argument(
        "--no-schema-fields",
        action="store_true",
        help="Do not pass the YAML field list to Tushare; inspect SDK default columns only.",
    )
    return parser.parse_args()


def load_sdk() -> tuple[Any, Any]:
    try:
        import pandas as pd  # type: ignore
        import tushare as ts  # type: ignore
    except ImportError as exc:
        raise SystemExit(
            "Missing development dependency. Install with: python -m pip install tushare pandas"
        ) from exc
    return pd, ts


def load_schema_fields(path: Path) -> dict[str, list[str]]:
    try:
        import yaml  # type: ignore
    except ImportError as exc:
        raise SystemExit("Missing PyYAML. Install with: python -m pip install pyyaml") from exc

    with path.open("r", encoding="utf-8") as handle:
        schema = yaml.safe_load(handle)
    datasets = schema.get("datasets", {}) if isinstance(schema, dict) else {}
    if not isinstance(datasets, dict):
        raise SystemExit(f"{path} must contain a top-level datasets mapping")

    field_map: dict[str, list[str]] = {}
    for dataset, meta in datasets.items():
        fields = meta.get("fields", []) if isinstance(meta, dict) else []
        if not isinstance(fields, list):
            continue
        names = []
        for field in fields:
            if isinstance(field, dict) and field.get("name"):
                names.append(str(field["name"]))
        field_map[str(dataset)] = names
    return field_map


def describe_frame(pd: Any, df: Any, max_samples: int) -> list[dict[str, Any]]:
    fields: list[dict[str, Any]] = []
    for column in df.columns:
        series = df[column]
        samples = []
        for value in series.dropna().head(max_samples).tolist():
            if pd.isna(value):
                samples.append(None)
            else:
                samples.append(value)
        fields.append(
            {
                "name": str(column),
                "pandas_dtype": str(series.dtype),
                "non_null": int(series.notna().sum()),
                "samples": samples,
            }
        )
    return fields


def inspect_dataset(
    pd: Any,
    pro: Any,
    dataset: str,
    args: argparse.Namespace,
    schema_fields: dict[str, list[str]],
) -> dict[str, Any]:
    spec = DATASETS[dataset]
    params = spec.build_params(args, pro)
    requested_fields = schema_fields.get(dataset, []) if not args.no_schema_fields else []
    if requested_fields:
        params["fields"] = ",".join(requested_fields)
    df = getattr(pro, spec.api)(**params)
    return {
        "dataset": dataset,
        "api": spec.api,
        "params": params,
        "requested_fields": requested_fields,
        "row_count": int(len(df)),
        "fields": describe_frame(pd, df, args.max_samples),
    }


def print_markdown(results: list[dict[str, Any]]) -> None:
    for result in results:
        print(f"## {result['dataset']}")
        print()
        print(f"API: `{result['api']}`")
        print(f"Rows: `{result['row_count']}`")
        if result.get("requested_fields"):
            print(f"Requested fields: `{','.join(result['requested_fields'])}`")
        print()
        print("| name | pandas_dtype | non_null | samples |")
        print("| --- | --- | ---: | --- |")
        for field in result["fields"]:
            samples = json.dumps(field["samples"], ensure_ascii=False, default=str)
            print(
                f"| `{field['name']}` | `{field['pandas_dtype']}` | {field['non_null']} | `{samples}` |"
            )
        print()


def main() -> int:
    args = parse_args()
    if bool(args.start_date) ^ bool(args.end_date):
        print("--start-date and --end-date must be provided together", file=sys.stderr)
        return 2

    token = os.environ.get("TUSHARE_TOKEN")
    if not token:
        print(
            "TUSHARE_TOKEN is not set. Export it before running this development helper.",
            file=sys.stderr,
        )
        return 2

    pd, ts = load_sdk()
    pro = ts.pro_api(token)
    dataset_ids = args.datasets or sorted(DATASETS)
    schema_fields = {} if args.no_schema_fields else load_schema_fields(args.schema)
    results = [inspect_dataset(pd, pro, dataset, args, schema_fields) for dataset in dataset_ids]

    if args.format == "json":
        print(json.dumps(results, ensure_ascii=False, indent=2, default=str))
    else:
        print_markdown(results)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
