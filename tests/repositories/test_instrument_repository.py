import pytest

from domain.models import Instrument
from repositories.instruments import (
    InstrumentNameAmbiguousError,
    InstrumentNotFoundError,
    InstrumentRepository,
)


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
    def __init__(self, rows: list[dict[str, object]]) -> None:
        self._rows = rows
        self.calls: list[tuple[str, str]] = []
        self.executemany_calls: list[tuple[str, list[tuple[object, ...]]]] = []

    async def fetch(self, query: str, name: str) -> list[dict[str, object]]:
        self.calls.append((query, name))
        return self._rows

    async def executemany(self, query: str, values: list[tuple[object, ...]]) -> None:
        self.executemany_calls.append((query, values))


@pytest.mark.asyncio
async def test_upsert_instruments_writes_industry() -> None:
    connection = _FakeConnection([])
    repository = InstrumentRepository(_FakePool(connection))

    await repository.upsert_instruments(
        [
            Instrument(
                ts_code="000001.SZ",
                symbol="000001",
                name="Ping An Bank",
                exchange="SZSE",
                list_status="L",
                is_st=False,
                industry="银行",
            )
        ]
    )

    query, values = connection.executemany_calls[0]
    assert "industry" in query
    assert values == [("000001.SZ", "000001", "Ping An Bank", "银行", "SZSE", "L", False)]


@pytest.mark.asyncio
async def test_find_by_name_returns_unique_instrument() -> None:
    connection = _FakeConnection(
        [
            {
                "ts_code": "000001.SZ",
                "symbol": "000001",
                "name": "Ping An Bank",
                "industry": "银行",
                "exchange": "SZSE",
                "list_status": "L",
                "is_st": False,
            }
        ]
    )

    instrument = await InstrumentRepository(_FakePool(connection)).find_by_name("Ping An Bank")

    assert instrument.ts_code == "000001.SZ"
    assert instrument.symbol == "000001"
    assert instrument.name == "Ping An Bank"
    assert instrument.industry == "银行"
    assert connection.calls[0][1] == "Ping An Bank"


@pytest.mark.asyncio
async def test_find_by_name_raises_when_instrument_missing() -> None:
    repository = InstrumentRepository(_FakePool(_FakeConnection([])))

    with pytest.raises(InstrumentNotFoundError, match="No instrument found"):
        await repository.find_by_name("Missing Name")


@pytest.mark.asyncio
async def test_find_by_name_raises_when_name_is_ambiguous() -> None:
    repository = InstrumentRepository(
        _FakePool(
            _FakeConnection(
                [
                    {
                        "ts_code": "000001.SZ",
                        "symbol": "000001",
                        "name": "Shared Name",
                        "exchange": "SZSE",
                        "list_status": "L",
                        "is_st": False,
                    },
                    {
                        "ts_code": "600001.SH",
                        "symbol": "600001",
                        "name": "Shared Name",
                        "exchange": "SSE",
                        "list_status": "L",
                        "is_st": False,
                    },
                ]
            )
        )
    )

    with pytest.raises(InstrumentNameAmbiguousError, match="Multiple instruments found"):
        await repository.find_by_name("Shared Name")
