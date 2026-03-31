import pytest

from use_cases.delete_by_date import DeleteByDateUseCase


class FakeMarketRepository:
    def __init__(self) -> None:
        self.calls: list[tuple[str, str]] = []

    async def delete_trade_date_range(self, start_date: str, end_date: str) -> dict[str, int]:
        self.calls.append((start_date, end_date))
        return {"daily_market_deleted": 12, "daily_indicators_deleted": 9}


@pytest.mark.asyncio
async def test_delete_by_date_returns_deleted_row_counts() -> None:
    repository = FakeMarketRepository()

    payload = await DeleteByDateUseCase(repository).run(start_date="20260101", end_date="20260131")

    assert payload == {
        "query": {"start_date": "2026-01-01", "end_date": "2026-01-31"},
        "data": {"daily_market_deleted": 12, "daily_indicators_deleted": 9},
        "meta": {"total_deleted_rows": 21},
    }
    assert repository.calls == [("20260101", "20260131")]
