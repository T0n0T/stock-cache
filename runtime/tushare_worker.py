#!/usr/bin/env python3
"""JSONL Tushare worker used by the Rust stock-cache CLI."""

from __future__ import annotations

import json
import os
import sys
import traceback
from typing import Any


BATCH_SIZE = 1000


def log(message: str) -> None:
    print(message, file=sys.stderr, flush=True)


def emit(message: dict[str, Any]) -> None:
    print(json.dumps(message, ensure_ascii=False, separators=(",", ":")), flush=True)


def error(req_id: str, message: str, retryable: bool = False) -> None:
    emit({"id": req_id, "type": "error", "message": message, "retryable": retryable})


def normalize_value(value: Any) -> Any:
    if value is None:
        return None
    try:
        import pandas as pd  # type: ignore

        if pd.isna(value):
            return None
    except Exception:
        pass
    if hasattr(value, "item"):
        try:
            value = value.item()
        except Exception:
            pass
    return value


def dataframe_rows(frame: Any) -> list[dict[str, Any]]:
    records = frame.to_dict(orient="records")
    return [{key: normalize_value(value) for key, value in row.items()} for row in records]


def call_tushare(pro: Any, dataset: str, api_name: str, fields: list[str], params: dict[str, Any]) -> list[dict[str, Any]]:
    method = getattr(pro, api_name, None)
    if method is not None:
        frame = method(fields=",".join(fields), **params)
    else:
        frame = pro.query(api_name, fields=",".join(fields), **params)
    return dataframe_rows(frame)


def handle_request(pro: Any, request: dict[str, Any]) -> None:
    req_id = request.get("id")
    if not isinstance(req_id, str) or not req_id:
        raise ValueError("request id must be a non-empty string")
    dataset = request.get("dataset")
    api_name = request.get("api")
    fields = request.get("fields")
    params = request.get("params", {})
    if not isinstance(dataset, str) or not dataset:
        error(req_id, "dataset must be a non-empty string", False)
        return
    if not isinstance(api_name, str) or not api_name:
        error(req_id, "api must be a non-empty string", False)
        return
    if not isinstance(fields, list) or not all(isinstance(field, str) for field in fields):
        error(req_id, "fields must be a list of strings", False)
        return
    if not isinstance(params, dict):
        error(req_id, "params must be an object", False)
        return

    try:
        rows = call_tushare(pro, dataset, api_name, fields, params)
        for offset in range(0, len(rows), BATCH_SIZE):
            emit({"id": req_id, "type": "batch", "rows": rows[offset : offset + BATCH_SIZE]})
        emit({"id": req_id, "type": "end", "row_count": len(rows)})
    except Exception as exc:
        log(traceback.format_exc())
        error(req_id, str(exc), True)


def main() -> int:
    token = os.environ.get("TUSHARE_TOKEN")
    if not token:
        log("TUSHARE_TOKEN is not set")
        return 2
    try:
        import tushare as ts  # type: ignore
    except Exception as exc:
        log(f"failed to import tushare: {exc}")
        return 2

    ts.set_token(token)
    pro = ts.pro_api(token)
    for line in sys.stdin:
        raw = line.strip()
        if not raw:
            continue
        try:
            request = json.loads(raw)
        except json.JSONDecodeError as exc:
            log(f"invalid request json: {exc}")
            return 2
        if not isinstance(request, dict):
            log("request must be a JSON object")
            return 2
        try:
            handle_request(pro, request)
        except Exception as exc:
            log(f"fatal protocol error: {exc}")
            return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
