import pytest

from stock_cache.db.pool import create_pool


@pytest.mark.asyncio
async def test_create_pool_connects_to_live_postgres(sample_dsn: str) -> None:
    pool = await create_pool(sample_dsn)

    async with pool.acquire() as connection:
        rows = await connection.fetch(
            "select tablename from pg_tables where schemaname = 'public' order by tablename"
        )

    await pool.close()

    assert [row["tablename"] for row in rows] == [
        "daily_indicators",
        "daily_market",
        "instruments",
        "job_runs",
    ]
