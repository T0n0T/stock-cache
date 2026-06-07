from datetime import date

import pytest

from domain.models import DailyCyqChipRow, DailyCyqPerfRow, DailyIndexRow, DailyIndicatorRow, DailyMarketRow
from use_cases.read_raw import ReadRawMarketDataUseCase


class FakeMarketRepository:
    async def fetch_raw(self, ts_code: str, start_date: str, end_date: str) -> dict[str, list[object]]:
        return {
            "market": [DailyMarketRow(ts_code=ts_code, trade_date=date(2026, 3, 30), close=12.4)],
            "indicators": [DailyIndicatorRow(ts_code=ts_code, trade_date=date(2026, 3, 30), macd=0.1)],
            "indexes": [],
            "cyq_chips": [DailyCyqChipRow(ts_code=ts_code, trade_date=date(2026, 3, 30), price=12.3)],
            "cyq_perf": [DailyCyqPerfRow(ts_code=ts_code, trade_date=date(2026, 3, 30), cost_50pct=11.8)],
        }


@pytest.mark.asyncio
async def test_read_raw_returns_json_ready_payload() -> None:
    payload = await ReadRawMarketDataUseCase(FakeMarketRepository()).run(
        ts_code="000001.SZ",
        start_date="2026-01-01",
        end_date="2026-03-30",
    )

    assert payload["query"]["ts_code"] == "000001.SZ"
    assert payload["meta"]["row_count_market"] == 1
    assert payload["data"]["market"][0]["close"] == 12.4
    assert payload["data"]["market"][0]["trade_date"] == "2026-03-30"
    assert payload["data"]["indicators"][0]["trade_date"] == "2026-03-30"
    assert payload["meta"]["row_count_indexes"] == 0
    assert payload["data"]["cyq_chips"][0]["price"] == 12.3
    assert payload["data"]["cyq_perf"][0]["cost_50pct"] == 11.8
    assert payload["meta"]["row_count_cyq_chips"] == 1
    assert payload["meta"]["row_count_cyq_perf"] == 1


class FakeIndexRepository:
    async def fetch_raw(self, ts_code: str, start_date: str, end_date: str) -> dict[str, list[object]]:
        return {
            "market": [],
            "indicators": [],
            "indexes": [
                DailyIndexRow(
                    ts_code=ts_code,
                    trade_date=date(2026, 4, 30),
                    name="国证2000",
                    group_name="major",
                    close=10989.9289,
                    pct_chg=0.5482,
                    source_daily="index_daily",
                )
            ],
            "cyq_chips": [],
            "cyq_perf": [],
        }


@pytest.mark.asyncio
async def test_read_raw_returns_index_rows_for_index_ts_code() -> None:
    payload = await ReadRawMarketDataUseCase(FakeIndexRepository()).run(
        ts_code="399303.SZ",
        start_date="2026-04-30",
        end_date="2026-04-30",
    )

    assert payload["query"]["ts_code"] == "399303.SZ"
    assert payload["meta"]["row_count_market"] == 0
    assert payload["meta"]["row_count_indicators"] == 0
    assert payload["meta"]["row_count_indexes"] == 1
    assert payload["data"]["indexes"][0]["name"] == "国证2000"
    assert payload["data"]["indexes"][0]["trade_date"] == "2026-04-30"
