import pytest

from use_cases.stats_date_range import StatsDateRangeUseCase


class FakeMarketRepository:
    async def fetch_trade_date_inventory(self) -> dict[str, list[str]]:
        return {
            "daily_market": ["2026-01-02", "2026-01-05", "2026-01-06", "2026-02-10"],
            "daily_indicators": ["2026-01-02", "2026-01-05", "2026-01-06"],
        }


class FakeTradeCalendarProvider:
    def __init__(self) -> None:
        self.calls: list[tuple[str, str]] = []

    def fetch_trade_dates_in_range(self, start_date: str, end_date: str) -> list[str]:
        self.calls.append((start_date, end_date))
        if (start_date, end_date) == ("20260102", "20260210"):
            return ["20260102", "20260105", "20260106", "20260107", "20260210"]
        return ["20260102", "20260105", "20260106"]


@pytest.mark.asyncio
async def test_stats_date_range_returns_continuous_trade_date_segments() -> None:
    provider = FakeTradeCalendarProvider()
    payload = await StatsDateRangeUseCase(FakeMarketRepository(), provider).run()

    assert payload["data"]["daily_market"] == {
        "min_trade_date": "2026-01-02",
        "max_trade_date": "2026-02-10",
        "continuous_ranges": [["2026-01-02", "2026-01-05", "2026-01-06"], ["2026-02-10"]],
    }
    assert payload["data"]["daily_indicators"] == {
        "min_trade_date": "2026-01-02",
        "max_trade_date": "2026-01-06",
        "continuous_ranges": [["2026-01-02", "2026-01-05", "2026-01-06"]],
    }
    assert provider.calls == [("20260102", "20260210"), ("20260102", "20260106")]


@pytest.mark.asyncio
async def test_stats_date_range_returns_empty_sections_when_cache_is_empty() -> None:
    class EmptyRepository:
        async def fetch_trade_date_inventory(self) -> dict[str, list[str]]:
            return {"daily_market": [], "daily_indicators": []}

    provider = FakeTradeCalendarProvider()
    payload = await StatsDateRangeUseCase(EmptyRepository(), provider).run()

    assert payload["data"]["daily_market"] == {
        "min_trade_date": None,
        "max_trade_date": None,
        "continuous_ranges": [],
    }
    assert payload["data"]["daily_indicators"] == {
        "min_trade_date": None,
        "max_trade_date": None,
        "continuous_ranges": [],
    }
    assert provider.calls == []
