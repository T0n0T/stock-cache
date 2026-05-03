from __future__ import annotations

import csv
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True, slots=True)
class IndexDefinition:
    ts_code: str
    name: str
    group_name: str
    enabled: bool


def load_index_definitions(path: Path) -> list[IndexDefinition]:
    with path.open(encoding="utf-8", newline="") as handle:
        reader = csv.DictReader(handle)
        rows = [
            IndexDefinition(
                ts_code=str(row["ts_code"]).strip(),
                name=str(row["name"]).strip(),
                group_name=str(row["group_name"]).strip(),
                enabled=_parse_enabled(row.get("enabled")),
            )
            for row in reader
        ]
    return [row for row in rows if row.enabled]


def _parse_enabled(value: object) -> bool:
    return str(value or "").strip().lower() in {"1", "true", "yes", "y", "on"}
