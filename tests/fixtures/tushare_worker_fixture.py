#!/usr/bin/env python3
from __future__ import annotations

import json
import sys


def rows_for(request):
    dataset = request["dataset"]
    params = request.get("params", {})
    trade_date = params.get("trade_date") or params.get("start_date") or "20260625"
    if dataset == "ths_index":
        return [
            {
                "ts_code": "885001.TI",
                "name": "fixture index",
                "count": 1,
                "exchange": "A",
                "list_date": "20200101",
                "type": "N",
            }
        ]
    if dataset == "ths_member":
        return [
            {
                "ts_code": params.get("ts_code", "885001.TI"),
                "con_code": "000001.SZ",
                "con_name": "fixture stock",
                "weight": None,
                "in_date": None,
                "out_date": None,
                "is_new": "Y",
            }
        ]
    if dataset == "stock_stk_factor_pro":
        return [{"ts_code": "000001.SZ", "trade_date": trade_date, "open": 1, "close_qfq": 1.2}]
    if dataset == "stock_moneyflow_ths":
        return [{"trade_date": trade_date, "ts_code": "000001.SZ", "name": "fixture", "net_amount": 1}]
    if dataset == "limit_list_ths":
        return [{"trade_date": trade_date, "ts_code": "000001.SZ", "name": "fixture", "price": 1}]
    if dataset == "limit_cpt_list":
        return [
            {
                "ts_code": "885001.TI",
                "name": "fixture",
                "trade_date": trade_date,
                "days": 1,
                "up_stat": "1/1",
                "cons_nums": 1,
                "up_nums": 1,
                "pct_chg": 1.23,
                "rank": 1,
            }
        ]
    if dataset == "index_idx_factor_pro":
        return [{"ts_code": params.get("ts_code", "000001.SH"), "trade_date": trade_date, "open": 1}]
    if dataset == "ths_daily":
        return [{"ts_code": params.get("ts_code", "885001.TI"), "trade_date": trade_date, "open": 1}]
    if dataset == "moneyflow_cnt_ths":
        return [{"trade_date": trade_date, "ts_code": "885001.TI", "name": "fixture", "company_num": 1}]
    return []


for line in sys.stdin:
    request = json.loads(line)
    req_id = request["id"]
    rows = rows_for(request)
    print(
        json.dumps(
            {
                "id": req_id,
                "type": "batch",
                "rows": rows,
            },
            separators=(",", ":"),
        ),
        flush=True,
    )
    print(json.dumps({"id": req_id, "type": "end", "row_count": len(rows)}, separators=(",", ":")), flush=True)
