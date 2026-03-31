from datetime import date
import json

import pytest

import repositories.market_data as market_data_module
from domain.models import DailyIndicatorRow, DailyMarketRow
from repositories.market_data import (
    MarketDataRepository,
    build_daily_indicator_upsert,
    build_daily_market_upsert,
)


def test_build_daily_market_upsert_uses_composite_key() -> None:
    row = DailyMarketRow(
        ts_code="000001.SZ",
        trade_date=date(2026, 3, 30),
        open=12.1,
        high=12.8,
        low=11.9,
        close=12.4,
        pre_close=12.0,
        change=0.4,
        pct_chg=2.5,
        vol=123456.0,
        amount=456789.0,
        turnover_rate=3.4,
        turnover_rate_f=3.1,
        volume_ratio=0.9,
        pe=10.0,
        pe_ttm=11.0,
        pb=1.2,
        ps=2.3,
        ps_ttm=2.4,
        dv_ratio=0.5,
        dv_ttm=0.6,
        total_share=1000.0,
        float_share=800.0,
        free_share=700.0,
        total_mv=100.5,
        circ_mv=80.5,
        net_mf_vol=6.7,
        net_mf_amount=8.9,
        source_provider="tushare",
        source_daily="daily",
        source_daily_basic="daily_basic",
        source_moneyflow="moneyflow",
        extra_market_jsonb={"adj_factor": 123.4, "up_limit": 13.75},
    )
    sql, values = build_daily_market_upsert([row])

    assert "ON CONFLICT (ts_code, trade_date)" in sql
    assert "pre_close" in sql
    assert "net_mf_vol" in sql
    assert "source_moneyflow" in sql
    assert "extra_market_jsonb" in sql
    assert "SET open = EXCLUDED.open" in sql
    assert "high = EXCLUDED.high" in sql
    assert "low = EXCLUDED.low" in sql
    assert values == [
        (
            "000001.SZ",
            date(2026, 3, 30),
            12.1,
            12.8,
            11.9,
            12.4,
            12.0,
            0.4,
            2.5,
            123456.0,
            456789.0,
            3.4,
            3.1,
            0.9,
            10.0,
            11.0,
            1.2,
            2.3,
            2.4,
            0.5,
            0.6,
            1000.0,
            800.0,
            700.0,
            100.5,
            80.5,
            6.7,
            8.9,
            json.dumps({"adj_factor": 123.4, "up_limit": 13.75}, ensure_ascii=False),
            "tushare",
            "daily",
            "daily_basic",
            "moneyflow",
        )
    ]


def test_build_daily_indicator_upsert_uses_composite_key() -> None:
    indicator = DailyIndicatorRow(
        ts_code="000001.SZ",
        trade_date=date(2026, 3, 30),
        macd=0.1,
        macd_dif=0.2,
        macd_dea=0.3,
        kdj_k=40.0,
        kdj_d=41.0,
        kdj_j=42.0,
        rsi_6=55.0,
        rsi_12=50.0,
        rsi_24=48.0,
        boll_upper=13.0,
        boll_mid=12.0,
        boll_lower=11.0,
        cci=101.0,
        source_provider="tushare",
        source_interface="stk_factor",
        calc_fallback_used=True,
        extra_factors_jsonb={"ema_qfq_5": 12.2, "wr_bfq": 17.1},
    )
    sql, values = build_daily_indicator_upsert([indicator])

    assert "ON CONFLICT (ts_code, trade_date)" in sql
    assert "rsi_6" in sql
    assert "boll_upper" in sql
    assert "cci" in sql
    assert "extra_factors_jsonb" in sql
    assert values == [
        (
            "000001.SZ",
            date(2026, 3, 30),
            0.1,
            0.2,
            0.3,
            40.0,
            41.0,
            42.0,
            55.0,
            50.0,
            48.0,
            13.0,
            12.0,
            11.0,
            101.0,
            json.dumps({"ema_qfq_5": 12.2, "wr_bfq": 17.1}, ensure_ascii=False),
            "tushare",
            "stk_factor",
            True,
        )
    ]


def test_build_daily_indicator_upsert_serializes_nan_as_null_in_json_payload() -> None:
    indicator = DailyIndicatorRow(
        ts_code="000001.SZ",
        trade_date=date(2026, 3, 30),
        extra_factors_jsonb={"ema_qfq_5": float("nan"), "wr_bfq": 17.1},
    )

    _, values = build_daily_indicator_upsert([indicator])

    assert values[0][15] == '{"ema_qfq_5": null, "wr_bfq": 17.1}'


class _FakeAcquire:
    def __init__(self, connection: object) -> None:
        self._connection = connection

    async def __aenter__(self) -> object:
        return self._connection

    async def __aexit__(self, exc_type: object, exc: object, tb: object) -> None:
        _ = (exc_type, exc, tb)


class _FakePool:
    def __init__(self, connection: object) -> None:
        self._connection = connection

    def acquire(self) -> _FakeAcquire:
        return _FakeAcquire(self._connection)


class _FakeConnection:
    def __init__(self) -> None:
        self.calls = 0

    async def fetch(self, query: str, ts_code: str, start: date, end: date) -> list[dict[str, object]]:
        _ = (query, ts_code, start, end)
        self.calls += 1
        if self.calls == 1:
            return [
                {
                    "ts_code": "000001.SZ",
                    "trade_date": date(2026, 3, 30),
                    "open": 12.1,
                    "high": 12.8,
                    "low": 11.9,
                    "close": 12.4,
                    "pct_chg": 2.5,
                    "pre_close": 12.0,
                    "change": 0.4,
                    "vol": 123456.0,
                    "amount": 456789.0,
                    "turnover_rate": 3.4,
                    "turnover_rate_f": 3.1,
                    "volume_ratio": 0.9,
                    "pe": 10.0,
                    "pe_ttm": 11.0,
                    "pb": 1.2,
                    "ps": 2.3,
                    "ps_ttm": 2.4,
                    "dv_ratio": 0.5,
                    "dv_ttm": 0.6,
                    "total_share": 1000.0,
                    "float_share": 800.0,
                    "free_share": 700.0,
                    "total_mv": 100.5,
                    "circ_mv": 80.5,
                    "net_mf_vol": 6.7,
                    "net_mf_amount": 8.9,
                    "extra_market_jsonb": '{"adj_factor": 123.4, "signals": ["limit_up"]}',
                    "source_provider": "tushare",
                    "source_daily": "daily",
                    "source_daily_basic": "daily_basic",
                    "source_moneyflow": "moneyflow",
                }
            ]
        return [
            {
                "ts_code": "000001.SZ",
                "trade_date": date(2026, 3, 30),
                "macd": 0.1,
                "macd_dif": 0.2,
                "macd_dea": 0.3,
                "kdj_k": 40.0,
                "kdj_d": 41.0,
                "kdj_j": 42.0,
                "rsi_6": 55.0,
                "rsi_12": 50.0,
                "rsi_24": 48.0,
                "boll_upper": 13.0,
                "boll_mid": 12.0,
                "boll_lower": 11.0,
                "cci": 101.0,
                "extra_factors_jsonb": '{"ema_qfq_5": 12.2, "wr_bfq": 17.1}',
                "source_provider": "tushare",
                "source_interface": "stk_factor_pro",
                "calc_fallback_used": False,
            }
        ]


@pytest.mark.asyncio
async def test_fetch_raw_decodes_jsonb_strings_from_postgres() -> None:
    repository = MarketDataRepository(_FakePool(_FakeConnection()))

    payload = await repository.fetch_raw("000001.SZ", "2026-03-30", "2026-03-30")

    assert payload["market"][0].extra_market_jsonb == {
        "adj_factor": 123.4,
        "signals": ["limit_up"],
    }
    assert payload["indicators"][0].extra_factors_jsonb == {
        "ema_qfq_5": 12.2,
        "wr_bfq": 17.1,
    }


class _WriteRecordingConnection:
    def __init__(self) -> None:
        self.executemany_calls: list[tuple[str, list[tuple[object, ...]]]] = []

    async def executemany(self, query: str, args: list[tuple[object, ...]]) -> None:
        self.executemany_calls.append((query, args))


def _market_rows(count: int) -> list[DailyMarketRow]:
    return [
        DailyMarketRow(
            ts_code=f"{i:06d}.SZ",
            trade_date=date(2026, 3, 30),
            extra_market_jsonb={"idx": i},
        )
        for i in range(count)
    ]


def _indicator_rows(count: int) -> list[DailyIndicatorRow]:
    return [
        DailyIndicatorRow(
            ts_code=f"{i:06d}.SZ",
            trade_date=date(2026, 3, 30),
            extra_factors_jsonb={"idx": i},
        )
        for i in range(count)
    ]


@pytest.mark.asyncio
async def test_upsert_daily_market_write_batches_splits_rows_by_batch_size(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    connection = _WriteRecordingConnection()
    repository = MarketDataRepository(_FakePool(connection), write_batch_size=2)
    rows = _market_rows(5)
    original_builder = market_data_module.build_daily_market_upsert
    builder_call_sizes: list[int] = []

    def _spy_builder(chunk_rows: list[DailyMarketRow]) -> tuple[str, list[tuple[object, ...]]]:
        builder_call_sizes.append(len(chunk_rows))
        return original_builder(chunk_rows)

    monkeypatch.setattr(market_data_module, "build_daily_market_upsert", _spy_builder)

    await repository.upsert_daily_market(rows)

    assert [len(args) for _, args in connection.executemany_calls] == [2, 2, 1]
    assert len({query for query, _ in connection.executemany_calls}) == 1
    assert builder_call_sizes == [2, 2, 1]
    assert [values[0] for _, args in connection.executemany_calls for values in args] == [
        row.ts_code for row in rows
    ]


@pytest.mark.asyncio
async def test_upsert_daily_indicators_write_batches_splits_rows_by_batch_size(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    connection = _WriteRecordingConnection()
    repository = MarketDataRepository(_FakePool(connection), write_batch_size=2)
    rows = _indicator_rows(5)
    original_builder = market_data_module.build_daily_indicator_upsert
    builder_call_sizes: list[int] = []

    def _spy_builder(chunk_rows: list[DailyIndicatorRow]) -> tuple[str, list[tuple[object, ...]]]:
        builder_call_sizes.append(len(chunk_rows))
        return original_builder(chunk_rows)

    monkeypatch.setattr(market_data_module, "build_daily_indicator_upsert", _spy_builder)

    await repository.upsert_daily_indicators(rows)

    assert [len(args) for _, args in connection.executemany_calls] == [2, 2, 1]
    assert len({query for query, _ in connection.executemany_calls}) == 1
    assert builder_call_sizes == [2, 2, 1]
    assert [values[0] for _, args in connection.executemany_calls for values in args] == [
        row.ts_code for row in rows
    ]


@pytest.mark.asyncio
async def test_upsert_daily_market_invalid_batch_size_falls_back_to_default_batches(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    connection = _WriteRecordingConnection()
    repository = MarketDataRepository(_FakePool(connection), write_batch_size=0)
    rows = _market_rows(501)
    original_builder = market_data_module.build_daily_market_upsert
    builder_call_sizes: list[int] = []

    def _spy_builder(chunk_rows: list[DailyMarketRow]) -> tuple[str, list[tuple[object, ...]]]:
        builder_call_sizes.append(len(chunk_rows))
        return original_builder(chunk_rows)

    monkeypatch.setattr(market_data_module, "build_daily_market_upsert", _spy_builder)

    await repository.upsert_daily_market(rows)

    assert [len(args) for _, args in connection.executemany_calls] == [500, 1]
    assert builder_call_sizes == [500, 1]


@pytest.mark.asyncio
async def test_upsert_daily_indicators_invalid_batch_size_falls_back_to_default_batches(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    connection = _WriteRecordingConnection()
    repository = MarketDataRepository(_FakePool(connection), write_batch_size=-7)
    rows = _indicator_rows(501)
    original_builder = market_data_module.build_daily_indicator_upsert
    builder_call_sizes: list[int] = []

    def _spy_builder(chunk_rows: list[DailyIndicatorRow]) -> tuple[str, list[tuple[object, ...]]]:
        builder_call_sizes.append(len(chunk_rows))
        return original_builder(chunk_rows)

    monkeypatch.setattr(market_data_module, "build_daily_indicator_upsert", _spy_builder)

    await repository.upsert_daily_indicators(rows)

    assert [len(args) for _, args in connection.executemany_calls] == [500, 1]
    assert builder_call_sizes == [500, 1]
