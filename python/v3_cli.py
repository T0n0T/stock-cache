#!/usr/bin/env python3
"""Thin Python orchestration layer for the Rust-owned v3 CLI."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from collections import defaultdict
from datetime import date, datetime
from pathlib import Path
from typing import Any


def parse_date(value: str) -> date:
    try:
        return date.fromisoformat(value)
    except ValueError:
        try:
            return datetime.strptime(value, "%Y%m%d").date()
        except ValueError as exc:
            raise argparse.ArgumentTypeError(f"invalid date: {value}") from exc


def _rust_binary(name: str, explicit: str | None = None) -> str:
    project_root = Path(__file__).resolve().parent.parent
    env_name = "STOCK_CACHE_V3_DB" if name == "v3-db" else "STOCK_CACHE_V3_CALCULATOR"
    candidates = [explicit, os.environ.get(env_name)]
    candidates.extend(
        [
            str(project_root / "target" / "release" / name),
            str(project_root / "target" / "debug" / name),
        ]
    )
    for candidate in candidates:
        if candidate and shutil.which(candidate):
            return candidate
        if candidate and Path(candidate).is_file() and os.access(candidate, os.X_OK):
            return candidate
    raise FileNotFoundError(
        f"{name} executable not found; build it with `cargo build --bin {name}`"
    )


def _run(command: list[str]) -> dict[str, Any]:
    try:
        completed = subprocess.run(command, check=True, capture_output=True, text=True)
    except subprocess.CalledProcessError as exc:
        detail = (exc.stderr or exc.stdout or "Rust v3 command failed").strip()
        raise RuntimeError(detail) from exc
    output = completed.stdout.strip()
    if not output:
        return {}
    try:
        return json.loads(output.splitlines()[-1])
    except json.JSONDecodeError as exc:
        raise RuntimeError(f"Rust v3 command returned invalid JSON: {output}") from exc


def command_init(args: argparse.Namespace) -> int:
    result = _run([_rust_binary("v3-db", args.db_binary), "init", "--database", args.database])
    print(json.dumps(result))
    return 0


def command_status(args: argparse.Namespace) -> int:
    result = _run([_rust_binary("v3-db", args.db_binary), "status", "--database", args.database])
    print(json.dumps(result))
    return 0


def _count_jsonl(path: str) -> int:
    if path == "-":
        return 0
    with Path(path).open(encoding="utf-8") as stream:
        return sum(bool(line.strip()) for line in stream)


def command_write(args: argparse.Namespace) -> int:
    command = [
        _rust_binary("v3-db", args.db_binary),
        "write",
        "--database",
        args.database,
        "--table",
        args.table,
        "--input",
        args.input,
        "--strategy",
        args.strategy,
    ]
    if args.dates:
        command.extend(["--dates", args.dates])
    for value in args.scope:
        command.extend(["--scope", value])
    if args.key_values:
        command.extend(["--key-values", args.key_values])
    result = _run(command)
    result["input_rows"] = _count_jsonl(args.input)
    print(json.dumps(result))
    return 0


def _normalise_dates(value: dict[str, Any]) -> dict[str, Any]:
    for key, field_value in list(value.items()):
        if (key == "trade_date" or key.endswith("_date")) and isinstance(field_value, str):
            value[key] = parse_date(field_value).isoformat()
    return value


def _read_calculated_rows(stream: str) -> dict[str, list[dict[str, Any]]]:
    result: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for line_number, line in enumerate(stream.splitlines(), 1):
        if not line.strip():
            continue
        envelope = json.loads(line)
        if not isinstance(envelope, dict) or not isinstance(envelope.get("table"), str):
            raise ValueError(f"calculator output line {line_number} is not a table envelope")
        row = envelope.get("row")
        if not isinstance(row, dict):
            raise ValueError(f"calculator output line {line_number} has no row object")
        result[envelope["table"]].append(_normalise_dates(row))
    return result


def _count_calculated_rows(path: str) -> dict[str, int]:
    result: dict[str, int] = defaultdict(int)
    with Path(path).open(encoding="utf-8") as stream:
        for line_number, line in enumerate(stream, 1):
            if not line.strip():
                continue
            envelope = json.loads(line)
            table = envelope.get("table") if isinstance(envelope, dict) else None
            if not isinstance(table, str):
                raise ValueError(f"calculator output line {line_number} is not a table envelope")
            result[table] += 1
    return dict(result)


def command_compute(args: argparse.Namespace) -> int:
    calculator = _rust_binary("v3-calc", args.calculator)
    command = [
        calculator,
        "--input",
        args.input,
        "--calendar",
        args.calendar,
        "--calc-version",
        args.calc_version,
        "--price-basis",
        args.price_basis,
        "--kdj-window",
        str(args.kdj_window),
    ]
    if args.source_dataset:
        command.extend(["--source-dataset", args.source_dataset])
    for value in args.scope:
        command.extend(["--scope", value])
    with tempfile.TemporaryDirectory(prefix="stock-cache-v3-calc-") as directory:
        output_path = Path(directory) / "calculated.jsonl"
        write_command = command + ["--database", args.database, "--write", "--output", str(output_path)]
        try:
            completed = subprocess.run(write_command, check=True, capture_output=True, text=True)
        except subprocess.CalledProcessError as exc:
            detail = (exc.stderr or exc.stdout or "calculator failed").strip()
            raise RuntimeError(detail) from exc
        table_counts = _count_calculated_rows(str(output_path))
    if not table_counts:
        print(json.dumps({"status": "ok", "input_rows": 0, "written_rows": {}}))
        return 0

    # The calculator itself owns the Rust writer in write mode.  Python only
    # parses the emitted envelopes to produce a compact command summary.
    print(json.dumps({"status": "ok", "strategy": "replace-batch-keys", "input_tables": table_counts, "written_rows": table_counts}))
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="stock-cache-v3")
    subparsers = parser.add_subparsers(dest="command", required=True)
    init = subparsers.add_parser("init")
    init.add_argument("--database", required=True)
    init.add_argument("--db-binary")
    init.set_defaults(function=command_init)
    status = subparsers.add_parser("status")
    status.add_argument("--database", required=True)
    status.add_argument("--db-binary")
    status.set_defaults(function=command_status)
    write = subparsers.add_parser("write")
    write.add_argument("--database", required=True)
    write.add_argument("--table", required=True)
    write.add_argument("--input", default="-", help="JSONL file, or - for stdin")
    write.add_argument("--strategy", required=True, choices=("append", "replace-dates", "replace-keys"))
    write.add_argument("--dates", help="comma-separated YYYY-MM-DD dates")
    write.add_argument("--scope", action="append", default=[], help="key=value; repeatable")
    write.add_argument("--key-values", help="JSON array of logical keys for replace-keys")
    write.add_argument("--db-binary")
    write.set_defaults(function=command_write)
    compute = subparsers.add_parser("compute")
    compute.add_argument("--database", required=True)
    compute.add_argument("--input", required=True, help="source_daily_bars JSONL")
    compute.add_argument("--calendar", required=True, help="open trading dates, one per line")
    compute.add_argument("--source-dataset")
    compute.add_argument("--calc-version", required=True)
    compute.add_argument("--price-basis", default="qfq")
    compute.add_argument("--kdj-window", type=int, default=9)
    compute.add_argument("--calculator")
    compute.add_argument("--db-binary")
    compute.add_argument("--scope", action="append", default=[], help="key=value; repeatable")
    compute.set_defaults(function=command_compute)
    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    try:
        return args.function(args)
    except Exception as exc:
        parser.error(str(exc))
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
