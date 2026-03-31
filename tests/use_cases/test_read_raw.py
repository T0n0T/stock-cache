from datetime import date

import pytest

from stock_cache.domain.models import DailyIndicatorRow, DailyMarketRow
from stock_cache.use_cases.read_raw import ReadRawMarketDataUseCase


class FakeMarketRepository:
    async def fetch_raw(self, ts_code: str, start_date: str, end_date: str) -> dict[str, list[object]]:
        return {
            "market": [DailyMarketRow(ts_code=ts_code, trade_date=date(2026, 3, 30), close=12.4)],
            "indicators": [DailyIndicatorRow(ts_code=ts_code, trade_date=date(2026, 3, 30), macd=0.1)],
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
