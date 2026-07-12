#!/usr/bin/env python3
"""Generate Batch A fixtures with the pinned CZSC implementation.

This is a maintainer-only tool. The generated JSON files are the test assets;
normal stock-cache builds and tests do not invoke this script or import CZSC.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from datetime import datetime, timedelta
from pathlib import Path
from typing import Any


SOURCE_COMMIT = "9ab62854f6bfab8515115b942baf4deb0f06185c"
SOURCE_VERSION = "1.0.0-rc.8"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--czsc-repo",
        required=True,
        type=Path,
        help="checkout of CZSC at the pinned commit",
    )
    parser.add_argument(
        "--real-source",
        required=True,
        type=Path,
        help="Eastmoney kline JSON fetched with fqt=1 (qfq)",
    )
    parser.add_argument(
        "--output-dir",
        default=Path("tests/fixtures/czsc"),
        type=Path,
    )
    return parser.parse_args()


def load_czsc(repo: Path):
    commit = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=repo,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    if commit != SOURCE_COMMIT:
        raise SystemExit(f"CZSC checkout is {commit}, expected {SOURCE_COMMIT}")

    sys.path.insert(0, str(repo))
    import czsc  # pylint: disable=import-outside-toplevel

    if czsc.__version__ != "1.0.0rc8":
        raise SystemExit(f"CZSC package is {czsc.__version__}, expected 1.0.0rc8")
    return czsc


def metadata(source_note: str) -> dict[str, Any]:
    return {
        "source_czsc_commit": SOURCE_COMMIT,
        "source_czsc_version": SOURCE_VERSION,
        "source_license": "Apache-2.0",
        "min_bi_len": 6,
        "max_bi_num": 50,
        "price_mode": "qfq",
        "source_note": source_note,
    }


def valid_ohlc(bar: dict[str, Any]) -> bool:
    values = [bar[key] for key in ("open", "high", "low", "close")]
    return (
        all(isinstance(value, (int, float)) and value > 0 for value in values)
        and bar["high"] >= max(bar["open"], bar["close"])
        and bar["low"] <= min(bar["open"], bar["close"])
        and bar["high"] >= bar["low"]
    )


def iso_date(value: Any) -> str:
    if hasattr(value, "date"):
        return value.date().isoformat()
    return str(value)[:10]


def bi_dict(bi: Any) -> dict[str, Any]:
    direction = str(bi.direction)
    return {
        "direction": {"向上": "up", "向下": "down"}[direction],
        "start_date": iso_date(bi.sdt),
        "end_date": iso_date(bi.edt),
        "high": bi.high,
        "low": bi.low,
    }


def get_zs_seq(czsc: Any, bis: list[Any]) -> list[Any]:
    """Exact port of crates/czsc-signals/src/utils/cxt.rs::get_zs_seq."""
    sequence = []
    for bi in bis:
        if not sequence:
            sequence.append(czsc.ZS([bi]))
            continue
        last = sequence.pop()
        if (bi.direction == czsc.Direction.Up and bi.high < last.zd) or (
            bi.direction == czsc.Direction.Down and bi.low > last.zg
        ):
            sequence.extend([last, czsc.ZS([bi])])
        else:
            sequence.append(czsc.ZS([*last.bis, bi]))
    return sequence


def zs_dict(zs: Any) -> dict[str, Any]:
    return {
        "start_date": iso_date(zs.sdt),
        "end_date": iso_date(zs.edt),
        "bi_count": len(zs.bis),
        "zg": zs.zg,
        "zd": zs.zd,
        "zz": zs.zz,
        "is_valid": zs.is_valid(),
    }


def structure_snapshot(
    czsc: Any,
    analyzer: Any,
    valid_bars: list[dict[str, Any]],
    source_bar_valid: bool,
) -> dict[str, Any]:
    finished = list(analyzer.finished_bis) if analyzer is not None else []
    recent = finished[-7:]
    last = finished[-1] if finished else None
    zss = get_zs_seq(czsc, finished)
    latest_zs = next(
        (zs for zs in reversed(zss) if len(zs.bis) >= 3 and zs.is_valid()),
        None,
    )

    raw_candidates = [raw for bi in recent for raw in bi.raw_bars]
    peak = max(raw_candidates, key=lambda raw: (raw.high, raw.dt), default=None)
    bear = None
    if peak is not None:
        peak_date = iso_date(peak.dt)
        bear = next(
            (
                bar
                for bar in valid_bars
                if bar["trade_date"] > peak_date and bar["close"] < bar["open"]
            ),
            None,
        )

    if not source_bar_valid:
        status = "invalid_source_bar"
    elif not finished:
        status = "no_finished_bi"
    elif len(finished) < 3:
        status = "insufficient_bis_for_zs"
    elif latest_zs is None:
        status = "no_valid_zs"
    else:
        status = "ok"

    last_high = last.high if last is not None else None
    last_low = last.low if last is not None else None
    return {
        "source_bar_valid": source_bar_valid,
        "status": status,
        "available_finished_bi_count": len(finished),
        "recent_bi_count": len(recent),
        "last_bi_direction": (
            {"向上": "up", "向下": "down"}[str(last.direction)] if last else None
        ),
        "last_bi_start_date": iso_date(last.sdt) if last else None,
        "last_bi_end_date": iso_date(last.edt) if last else None,
        "last_bi_high": last_high,
        "last_bi_low": last_low,
        "last_bi_mid": (last_high + last_low) / 2 if last else None,
        "latest_zs_start_date": iso_date(latest_zs.sdt) if latest_zs else None,
        "latest_zs_end_date": iso_date(latest_zs.edt) if latest_zs else None,
        "latest_zs_bi_count": len(latest_zs.bis) if latest_zs else None,
        "latest_zs_zg": latest_zs.zg if latest_zs else None,
        "latest_zs_zd": latest_zs.zd if latest_zs else None,
        "latest_zs_zz": latest_zs.zz if latest_zs else None,
        "recent_peak_date": iso_date(peak.dt) if peak else None,
        "recent_peak_high": peak.high if peak else None,
        "post_peak_first_bear_date": bear["trade_date"] if bear else None,
        "post_peak_first_bear_open": bear["open"] if bear else None,
        "post_peak_first_bear_high": bear["high"] if bear else None,
        "post_peak_first_bear_low": bear["low"] if bear else None,
        "post_peak_first_bear_close": bear["close"] if bear else None,
        "post_peak_first_bear_missing": None if peak is None else bear is None,
    }


def raw_bar(czsc: Any, bar: dict[str, Any], bar_id: int) -> Any:
    dt = datetime.fromisoformat(bar["trade_date"])
    return czsc.RawBar(
        "ANON",
        dt,
        czsc.Freq.D,
        bar["open"],
        bar["close"],
        bar["high"],
        bar["low"],
        0.0,
        0.0,
        bar_id,
    )


def expected_days(czsc: Any, bars: list[dict[str, Any]]) -> list[dict[str, Any]]:
    analyzer = None
    previous_bis: list[dict[str, Any]] = []
    removed_last_bis: list[dict[str, Any]] = []
    expected = []
    valid_bars = []
    valid_id = 0
    for bar in bars:
        source_bar_valid = valid_ohlc(bar)
        if source_bar_valid:
            valid_bars.append(bar)
            raw = raw_bar(czsc, bar, valid_id)
            valid_id += 1
            if analyzer is None:
                analyzer = czsc.CZSC([raw], 50, 6)
            else:
                analyzer.update(raw)

        bi_list = list(analyzer.bi_list) if analyzer is not None else []
        finished = list(analyzer.finished_bis) if analyzer is not None else []
        bis = [bi_dict(bi) for bi in bi_list]
        events = []
        if source_bar_valid and len(bis) < len(previous_bis):
            events.extend(["last_bi_break", "last_bi_removal"])
            removed_last_bis.append(previous_bis[-1])
        if source_bar_valid and bis:
            last = bis[-1]
            for removed in reversed(removed_last_bis):
                if (
                    removed["direction"] == last["direction"]
                    and removed["start_date"] == last["start_date"]
                ):
                    extended = (
                        last["direction"] == "up"
                        and (last["end_date"] > removed["end_date"] or last["high"] > removed["high"])
                    ) or (
                        last["direction"] == "down"
                        and (last["end_date"] > removed["end_date"] or last["low"] < removed["low"])
                    )
                    if extended:
                        events.append("last_bi_extension")
                        removed_last_bis.remove(removed)
                    break
        if source_bar_valid and bi_list:
            events.append(
                "finished_tail_included"
                if len(finished) == len(bi_list)
                else "finished_tail_excluded"
            )

        expected.append(
            {
                "trade_date": bar["trade_date"],
                "source_bar_valid": source_bar_valid,
                "bars_ubi_count": len(analyzer.bars_ubi) if analyzer is not None else 0,
                "bi_list_count": len(bi_list),
                "bi_list": bis,
                "finished_bis": [bi_dict(bi) for bi in finished],
                "zs_seq": [zs_dict(zs) for zs in get_zs_seq(czsc, finished)],
                "snapshot": structure_snapshot(
                    czsc, analyzer, valid_bars, source_bar_valid
                ),
                "events": events,
            }
        )
        previous_bis = bis
    return expected


def case(
    czsc: Any,
    case_id: str,
    origin: str,
    covers: list[str],
    bars: list[dict[str, Any]],
) -> dict[str, Any]:
    return {
        "id": case_id,
        "origin": origin,
        "covers": covers,
        "bars": bars,
        "expected": expected_days(czsc, bars),
    }


def market_bars(path: Path) -> list[dict[str, Any]]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    if payload.get("rc") != 0 or not isinstance(payload.get("data"), dict):
        raise SystemExit("real source is not a successful Eastmoney K-line response")
    rows = payload["data"]["klines"]
    if len(rows) < 160:
        raise SystemExit("real source must contain at least 160 daily bars")
    parsed = []
    for row in rows:
        trade_date, open_, close, high, low, *_ = row.split(",")
        parsed.append(
            {
                "trade_date": trade_date,
                "open": float(open_),
                "high": float(high),
                "low": float(low),
                "close": float(close),
            }
        )
    # A compact window with enough history for several complete BI transitions.
    return parsed[-160:]


def trading_dates(count: int, start: str = "2024-01-02") -> list[str]:
    dates = []
    current = datetime.fromisoformat(start)
    while len(dates) < count:
        if current.weekday() < 5:
            dates.append(current.date().isoformat())
        current += timedelta(days=1)
    return dates


def trend_bars(start: float, step: float, count: int) -> list[dict[str, Any]]:
    bars = []
    for trade_date, index in zip(trading_dates(count), range(count)):
        close = start + step * index
        open_ = close - step * 0.25
        bars.append(
            {
                "trade_date": trade_date,
                "open": round(open_, 4),
                "high": round(max(open_, close) + 0.3, 4),
                "low": round(min(open_, close) - 0.3, 4),
                "close": round(close, 4),
            }
        )
    return bars


def swing_bars() -> list[dict[str, Any]]:
    targets = [10, 16, 9, 17, 8, 18, 7, 19, 11, 20, 10, 21, 9]
    closes = []
    for start, end in zip(targets, targets[1:]):
        for step in range(7):
            closes.append(start + (end - start) * step / 7)
    closes.append(targets[-1])
    dates = trading_dates(len(closes) + 3)
    # A multi-week date hole represents suspension; no synthetic bars are inserted.
    dates = dates[:38] + dates[48 : 48 + len(closes) - 38]
    bars = []
    for index, (trade_date, close) in enumerate(zip(dates, closes)):
        open_ = close + (0.18 if index % 3 == 0 else -0.12)
        bars.append(
            {
                "trade_date": trade_date,
                "open": round(open_, 4),
                "high": round(max(open_, close) + 0.45, 4),
                "low": round(min(open_, close) - 0.45, 4),
                "close": round(close, 4),
            }
        )

    # Explicit inclusion, price gap, tied highs, doji, and invalid OHLC bars.
    bars[12].update(open=11.2, high=16.8, low=8.8, close=14.9)
    bars[13].update(open=12.0, high=15.8, low=9.6, close=13.0)
    bars[38].update(open=18.8, high=19.4, low=18.5, close=19.1)
    bars[39].update(open=20.2, high=21.0, low=20.0, close=20.7)
    bars[40].update(open=20.5, high=21.0, low=19.9, close=20.1)
    bars[41].update(open=20.4, high=20.8, low=20.0, close=20.4)
    bars[42].update(open=19.8, high=19.7, low=19.2, close=19.5)
    return bars


def write_fixture(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(payload, indent=2, ensure_ascii=True) + "\n",
        encoding="utf-8",
    )


def main() -> None:
    args = parse_args()
    czsc = load_czsc(args.czsc_repo.resolve())

    real = {
        "schema_version": 1,
        "metadata": metadata(
            "Public A-share daily bars fetched from Eastmoney with fqt=1; "
            "security identity removed and only qfq OHLC/date retained."
        ),
        "cases": [
            case(
                czsc,
                "real_market_anon_001",
                "anonymized_real_market",
                ["real_market_point_in_time"],
                market_bars(args.real_source),
            )
        ],
    }
    handcrafted = {
        "schema_version": 1,
        "metadata": metadata("Deterministic handcrafted OHLC scenarios."),
        "cases": [
            case(czsc, "monotone_up", "handcrafted", ["monotone_up"], trend_bars(10, 0.5, 20)),
            case(
                czsc,
                "monotone_down",
                "handcrafted",
                ["monotone_down"],
                trend_bars(20, -0.5, 20),
            ),
            case(
                czsc,
                "swing_lifecycle",
                "handcrafted",
                [
                    "wide_oscillation",
                    "bar_inclusion",
                    "gap",
                    "resume_after_suspension",
                    "tied_high",
                    "doji",
                    "invalid_ohlc",
                    "last_bi_extension",
                    "last_bi_break",
                    "last_bi_removal",
                ],
                swing_bars(),
            ),
        ],
    }
    write_fixture(args.output_dir / "anonymized_real_daily.json", real)
    write_fixture(args.output_dir / "handcrafted_daily.json", handcrafted)


if __name__ == "__main__":
    main()
