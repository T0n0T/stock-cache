import asyncpg


class DatabasePrecheckError(RuntimeError):
    pass


async def create_pool(dsn: str) -> asyncpg.Pool:
    try:
        pool = await asyncpg.create_pool(dsn=dsn, min_size=1, max_size=5)
        async with pool.acquire() as connection:
            await connection.execute("select 1")
    except (asyncpg.PostgresError, OSError) as exc:
        pool = locals().get("pool")
        if pool is not None:
            await pool.close()
        raise DatabasePrecheckError("PostgreSQL is not reachable at configured POSTGRES_DSN.") from exc
    return pool
