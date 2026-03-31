import asyncpg

from domain.models import Instrument


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
            exchange,
            list_status,
            is_st
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (ts_code) DO UPDATE
        SET symbol = EXCLUDED.symbol,
            name = EXCLUDED.name,
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
                instrument.exchange,
                instrument.list_status,
                instrument.is_st,
            )
            for instrument in instruments
        ]
        async with self._pool.acquire() as connection:
            await connection.executemany(sql, values)
