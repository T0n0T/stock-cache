import asyncpg

from domain.models import Instrument


class InstrumentLookupError(LookupError):
    pass


class InstrumentNotFoundError(InstrumentLookupError):
    def __init__(self, name: str) -> None:
        super().__init__(f"No instrument found for --name '{name}'.")


class InstrumentNameAmbiguousError(InstrumentLookupError):
    def __init__(self, name: str) -> None:
        super().__init__(f"Multiple instruments found for --name '{name}'. Use --ts-code instead.")


class InstrumentRepository:
    def __init__(self, pool: asyncpg.Pool) -> None:
        self._pool = pool

    async def upsert_instruments(self, instruments: list[Instrument]) -> None:
        if not instruments:
            return

        sql = """
        INSERT INTO instruments (
            ts_code,
            symbol,
            name,
            industry,
            exchange,
            list_status,
            is_st
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (ts_code) DO UPDATE
        SET symbol = EXCLUDED.symbol,
            name = EXCLUDED.name,
            industry = EXCLUDED.industry,
            exchange = EXCLUDED.exchange,
            list_status = EXCLUDED.list_status,
            is_st = EXCLUDED.is_st,
            updated_at = NOW()
        """
        values = [
            (
                instrument.ts_code,
                instrument.symbol,
                instrument.name,
                instrument.industry,
                instrument.exchange,
                instrument.list_status,
                instrument.is_st,
            )
            for instrument in instruments
        ]
        async with self._pool.acquire() as connection:
            await connection.executemany(sql, values)

    async def find_by_name(self, name: str) -> Instrument:
        sql = """
        SELECT ts_code, symbol, name, industry, exchange, list_status, is_st
        FROM instruments
        WHERE name = $1
        ORDER BY ts_code
        LIMIT 2
        """
        async with self._pool.acquire() as connection:
            rows = await connection.fetch(sql, name)

        if not rows:
            raise InstrumentNotFoundError(name)
        if len(rows) > 1:
            raise InstrumentNameAmbiguousError(name)

        row = rows[0]
        return Instrument(
            ts_code=row["ts_code"],
            symbol=row["symbol"],
            name=row["name"],
            exchange=row["exchange"],
            list_status=row["list_status"],
            is_st=row["is_st"],
            industry=row["industry"],
        )
