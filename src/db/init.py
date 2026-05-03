from pathlib import Path

import asyncpg


REQUIRED_TABLES = (
    "daily_index",
    "daily_indicators",
    "daily_market",
    "instruments",
    "job_runs",
)


def _schema_path() -> Path:
    return Path(__file__).with_name("schema.sql")


async def list_public_tables(connection: asyncpg.Connection) -> list[str]:
    rows = await connection.fetch(
        "select tablename from pg_tables where schemaname = 'public' order by tablename"
    )
    return [row["tablename"] for row in rows]


async def initialize_schema(pool: asyncpg.Pool) -> dict[str, object]:
    async with pool.acquire() as connection:
        before_tables = set(await list_public_tables(connection))
        await connection.execute(_schema_path().read_text(encoding="utf-8"))
        after_tables = set(await list_public_tables(connection))

    required = set(REQUIRED_TABLES)
    created_tables = sorted((after_tables - before_tables) & required)
    already_present = sorted(before_tables & required)
    missing = sorted(required - after_tables)
    return {
        "status": "ok" if not missing else "failed",
        "created_tables": created_tables,
        "already_present": already_present,
        "missing": missing,
    }
