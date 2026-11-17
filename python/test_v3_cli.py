from __future__ import annotations

import contextlib
import io
import json
import sys
import tempfile
import unittest
from datetime import date, timedelta
from pathlib import Path

import duckdb

ROOT = Path(__file__).resolve().parent.parent
if str(Path(__file__).resolve().parent) not in sys.path:
    sys.path.insert(0, str(Path(__file__).resolve().parent))

import v3_cli


class V3CliTests(unittest.TestCase):
    def run_cli(self, *arguments: str) -> dict:
        output = io.StringIO()
        old_argv = sys.argv
        try:
            sys.argv = ["stock-cache-v3", *arguments]
            with contextlib.redirect_stdout(output):
                result = v3_cli.main()
        finally:
            sys.argv = old_argv
        self.assertEqual(result, 0)
        return json.loads(output.getvalue())

    def write_jsonl(self, path: Path, rows: list[dict]) -> None:
        path.write_text("".join(json.dumps(row) + "\n" for row in rows), encoding="utf-8")

    def source_row(self, trade_date: str, code: str = "000001.SZ", close: float = 10.0) -> dict:
        return {
            "source_dataset": "source_daily_bars",
            "ts_code": code,
            "trade_date": trade_date,
            "open": close - 0.5,
            "high": close + 0.5,
            "low": close - 1.0,
            "close": close,
            "vol": 100.0,
            "amount": 1000.0,
            "price_basis": "qfq",
        }

    def test_init_status_and_source_write_strategies(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            database = root / "v3.duckdb"
            rows = [self.source_row("2025-01-02", close=10), self.source_row("2025-01-03", close=11)]
            first = root / "first.jsonl"
            self.write_jsonl(first, rows)
            initialized = self.run_cli("init", "--database", str(database))
            self.assertEqual(initialized["status"], "ok")
            written = self.run_cli(
                "write",
                "--database",
                str(database),
                "--table",
                "source_daily_bars",
                "--input",
                str(first),
                "--strategy",
                "append",
            )
            self.assertEqual(written["written_rows"], 2)
            status = self.run_cli("status", "--database", str(database))
            self.assertEqual(status["tables"]["source_daily_bars"], 2)
            with duckdb.connect(str(database), read_only=True) as connection:
                self.assertIsNone(connection.execute("SELECT turnover_rate FROM source_daily_bars WHERE trade_date = DATE '2025-01-02'").fetchone()[0])

            replacement = root / "replacement.jsonl"
            self.write_jsonl(replacement, [self.source_row("2025-01-02", close=99)])
            self.run_cli(
                "write",
                "--database",
                str(database),
                "--table",
                "source_daily_bars",
                "--input",
                str(replacement),
                "--strategy",
                "replace-dates",
                "--dates",
                "2025-01-02",
            )
            with duckdb.connect(str(database), read_only=True) as connection:
                self.assertEqual(connection.execute("SELECT close FROM source_daily_bars WHERE trade_date = DATE '2025-01-02'").fetchone()[0], 99)

    def test_replace_dates_empty_and_replace_keys_delete(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            database = root / "v3.duckdb"
            self.run_cli("init", "--database", str(database))
            source = root / "source.jsonl"
            self.write_jsonl(source, [self.source_row("2025-01-02"), self.source_row("2025-01-03")])
            self.run_cli("write", "--database", str(database), "--table", "source_daily_bars", "--input", str(source), "--strategy", "append")
            empty = root / "empty.jsonl"
            empty.write_text("", encoding="utf-8")
            self.run_cli("write", "--database", str(database), "--table", "source_daily_bars", "--input", str(empty), "--strategy", "replace-dates", "--dates", "2025-01-02")
            self.run_cli("write", "--database", str(database), "--table", "source_daily_bars", "--input", str(empty), "--strategy", "replace-keys", "--key-values", '[["source_daily_bars","000001.SZ","2025-01-03"]]')
            with duckdb.connect(str(database), read_only=True) as connection:
                self.assertEqual(connection.execute("SELECT COUNT(*) FROM source_daily_bars").fetchone()[0], 0)

    def test_compute_runs_rust_kernels_and_writes_calculation_tables(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            database = root / "v3.duckdb"
            self.run_cli("init", "--database", str(database))
            bars_path = root / "bars.jsonl"
            calendar_path = root / "calendar.txt"
            start = date(2025, 1, 2)
            dates = [start + timedelta(days=offset) for offset in range(35) if (start + timedelta(days=offset)).weekday() < 5]
            rows = [self.source_row(day.isoformat(), close=10 + index) for index, day in enumerate(dates)]
            self.write_jsonl(bars_path, rows)
            calendar_path.write_text("".join(day.isoformat() + "\n" for day in dates), encoding="utf-8")
            result = self.run_cli(
                "compute",
                "--database",
                str(database),
                "--input",
                str(bars_path),
                "--calendar",
                str(calendar_path),
                "--calc-version",
                "v3-test",
                "--calculator",
                str(ROOT / "target" / "debug" / "v3-calc"),
            )
            self.assertEqual(result["strategy"], "replace-batch-keys")
            self.assertEqual(result["written_rows"]["calc_daily_indicators"], len(rows))
            with duckdb.connect(str(database), read_only=True) as connection:
                self.assertEqual(connection.execute("SELECT COUNT(*) FROM calc_daily_czsc_structure").fetchone()[0], len(rows))
                self.assertEqual(connection.execute("SELECT COUNT(*) FROM calc_daily_indicators WHERE calc_version = 'v3-test'").fetchone()[0], len(rows))
                self.assertEqual(connection.execute("SELECT COUNT(*) FROM calc_indicator_state WHERE period_type = 'daily'").fetchone()[0], len(rows))


if __name__ == "__main__":
    unittest.main()
