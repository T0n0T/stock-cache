#!/usr/bin/env python3
"""Measure the Rust DuckDB writer across representative write workloads."""

from __future__ import annotations

import argparse
import json
import platform
import statistics
import subprocess
import tempfile
import time
from datetime import date, timedelta, datetime, timezone
from pathlib import Path


def rows_for(start: date, days: int, rows_per_day: int) -> list[dict[str, object]]:
    rows: list[dict[str, object]] = []
    for offset in range(days):
        current = start + timedelta(days=offset)
        for symbol in range(rows_per_day):
            close = float(symbol) + offset / 100.0
            rows.append(
                {
                    "ts_code": f"{symbol:06d}.SZ",
                    "trade_date": current.isoformat(),
                    "rank": symbol + 1,
                }
            )
    return rows


def write_jsonl(path: Path, rows: list[dict[str, object]]) -> None:
    with path.open("w", encoding="utf-8") as stream:
        for row in rows:
            stream.write(json.dumps(row, separators=(",", ":")) + "\n")


def run(binary: str, database: Path, command: list[str]) -> dict[str, object]:
    completed = subprocess.run(
        [binary, *command, "--database", str(database)],
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(completed.stdout.strip().splitlines()[-1])


def measure(binary: str, workload: str, rows_per_day: int, root: Path) -> dict[str, object]:
    root.mkdir(parents=True, exist_ok=True)
    start = date(2025, 1, 2)
    if workload == "daily_append":
        base, batch, strategy, dates = rows_for(start, 30, rows_per_day), rows_for(start + timedelta(days=30), 1, rows_per_day), "append", None
    elif workload == "daily_replace":
        base, batch, strategy, dates = rows_for(start, 30, rows_per_day), rows_for(start + timedelta(days=29), 1, rows_per_day), "replace-dates", [start + timedelta(days=29)]
    elif workload == "90d_replace":
        base, batch, strategy, dates = rows_for(start, 180, rows_per_day), rows_for(start + timedelta(days=60), 90, rows_per_day), "replace-dates", [start + timedelta(days=offset) for offset in range(60, 150)]
    elif workload == "full_build":
        base, batch, strategy, dates = [], rows_for(start, 180, rows_per_day), "append", None
    else:
        raise ValueError(workload)
    base_path, batch_path = root / f"{workload}-base.jsonl", root / f"{workload}-batch.jsonl"
    write_jsonl(base_path, base)
    write_jsonl(batch_path, batch)
    database = root / f"{workload}.duckdb"
    run(binary, database, ["init"])
    if base:
        run(binary, database, ["import", "--table", "index_limit_cpt_list", "--input", str(base_path), "--strategy", "append"])
    command = ["import", "--table", "index_limit_cpt_list", "--input", str(batch_path), "--strategy", strategy]
    if dates:
        command.extend(["--dates", ",".join(value.isoformat() for value in dates)])
    started = time.perf_counter()
    result = run(binary, database, command)
    elapsed_ms = (time.perf_counter() - started) * 1000
    return {"workload": workload, "input_rows": len(batch), "written_rows": result["written_rows"], "write_ms": round(elapsed_ms, 3), "rows_per_sec": round(len(batch) / (elapsed_ms / 1000), 2), "database_bytes": database.stat().st_size}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", default="target/release/examples/writer-benchmark")
    parser.add_argument("--rows-per-day", type=int, default=50)
    parser.add_argument("--runs", type=int, default=5)
    default_output = Path(__file__).resolve().parents[1] / "target" / "stock-cache-writer-benchmark.json"
    parser.add_argument("--output", type=Path, default=default_output)
    args = parser.parse_args()
    trials: list[dict[str, object]] = []
    with tempfile.TemporaryDirectory(prefix="stock-cache-bench-") as directory:
        root = Path(directory)
        for workload in ("daily_append", "daily_replace", "90d_replace", "full_build"):
            for run_number in range(1, args.runs + 1):
                trial = measure(args.binary, workload, args.rows_per_day, root / f"{workload}-{run_number}")
                trial["run"] = run_number
                trials.append(trial)
    summary = []
    for workload in ("daily_append", "daily_replace", "90d_replace", "full_build"):
        values = [float(row["write_ms"]) for row in trials if row["workload"] == workload]
        rows = next(row["input_rows"] for row in trials if row["workload"] == workload)
        summary.append({"workload": workload, "runs": len(values), "input_rows": rows, "write_ms_median": round(statistics.median(values), 3), "write_ms_p95": round(max(values), 3), "rows_per_sec_median": round(rows / (statistics.median(values) / 1000), 2)})
    payload = {"metadata": {"generated_at_utc": datetime.now(timezone.utc).isoformat(), "binary": args.binary, "rows_per_day": args.rows_per_day, "runs": args.runs, "platform": platform.platform()}, "summary": summary, "trials": trials}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"output": str(args.output), "summary": summary}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
