from datetime import date

import pytest

from repositories.market_data import MarketDataRepository


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


class _InventoryConnection:
    def __init__(self) -> None:
        self.calls: list[str] = []

    async def fetch(self, query: str) -> list[dict[str, object]]:
        self.calls.append(query)
        if "from daily_market" in query.lower():
            return [
                {"trade_date": date(2026, 1, 2)},
                {"trade_date": date(2026, 1, 5)},
            ]
        if "from daily_cyq_chips" in query.lower():
            return [
                {"trade_date": date(2026, 1, 2)},
                {"trade_date": date(2026, 1, 5)},
            ]
        if "from daily_cyq_perf" in query.lower():
            return [{"trade_date": date(2026, 1, 5)}]
        return [{"trade_date": date(2026, 1, 2)}]


class _DeleteConnection:
    def __init__(self) -> None:
        self.calls: list[tuple[str, date, date]] = []

    async def execute(self, query: str, start_date: date, end_date: date) -> str:
        self.calls.append((query, start_date, end_date))
        if "delete from daily_market" in query.lower():
            return "DELETE 12"
        if "delete from daily_indicators" in query.lower():
            return "DELETE 9"
        if "delete from daily_index" in query.lower():
            return "DELETE 3"
        if "delete from daily_cyq_chips" in query.lower():
            return "DELETE 30"
        if "delete from daily_cyq_perf" in query.lower():
            return "DELETE 6"
        return "DELETE 9"


@pytest.mark.asyncio
async def test_fetch_trade_date_inventory_returns_sorted_iso_dates() -> None:
    repository = MarketDataRepository(_FakePool(_InventoryConnection()))

    payload = await repository.fetch_trade_date_inventory()

    assert payload == {
        "daily_market": ["2026-01-02", "2026-01-05"],
        "daily_indicators": ["2026-01-02"],
        "daily_index": ["2026-01-02"],
        "daily_cyq_chips": ["2026-01-02", "2026-01-05"],
        "daily_cyq_perf": ["2026-01-05"],
    }


@pytest.mark.asyncio
async def test_delete_trade_date_range_returns_deleted_row_counts() -> None:
    connection = _DeleteConnection()
    repository = MarketDataRepository(_FakePool(connection))

    payload = await repository.delete_trade_date_range("20260101", "20260131")

    assert payload == {
        "daily_market_deleted": 12,
        "daily_indicators_deleted": 9,
        "daily_index_deleted": 3,
        "daily_cyq_chips_deleted": 30,
        "daily_cyq_perf_deleted": 6,
    }
    assert connection.calls[0][1:] == (date(2026, 1, 1), date(2026, 1, 31))
    assert connection.calls[1][1:] == (date(2026, 1, 1), date(2026, 1, 31))
    assert connection.calls[2][1:] == (date(2026, 1, 1), date(2026, 1, 31))
    assert connection.calls[3][1:] == (date(2026, 1, 1), date(2026, 1, 31))
    assert connection.calls[4][1:] == (date(2026, 1, 1), date(2026, 1, 31))
