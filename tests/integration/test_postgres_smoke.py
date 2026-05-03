import json
import os
import subprocess
import sys

import pytest

from db.pool import create_pool


CORE_TABLES = [
    "daily_index",
    "daily_indicators",
    "daily_market",
    "instruments",
    "job_runs",
]


async def _list_public_tables(sample_dsn: str) -> list[str]:
    pool = await create_pool(sample_dsn)
    try:
        async with pool.acquire() as connection:
            rows = await connection.fetch(
                "select tablename from pg_tables where schemaname = 'public' order by tablename"
            )
    finally:
        await pool.close()
    return [row["tablename"] for row in rows]


async def _drop_core_tables(sample_dsn: str) -> None:
    pool = await create_pool(sample_dsn)
    try:
        async with pool.acquire() as connection:
            for table_name in reversed(CORE_TABLES):
                await connection.execute(f"drop table if exists {table_name}")
    finally:
        await pool.close()


def _run_init_db(sample_dsn: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, "-m", "cli", "init-db"],
        capture_output=True,
        check=False,
        env={
            "PATH": os.environ["PATH"],
            "POSTGRES_DSN": sample_dsn,
            "PYTHONPATH": "src",
            "TUSHARE_TOKEN": "token",
        },
        text=True,
        timeout=5,
    )


@pytest.mark.asyncio
async def test_create_pool_connects_to_live_postgres(sample_dsn: str) -> None:
    assert await _list_public_tables(sample_dsn) == CORE_TABLES


@pytest.mark.asyncio
async def test_init_db_command_creates_missing_core_tables(sample_dsn: str) -> None:
    await _drop_core_tables(sample_dsn)
    assert await _list_public_tables(sample_dsn) == []

    result = _run_init_db(sample_dsn)

    assert result.returncode == 0
    payload = json.loads(result.stdout)
    assert payload["status"] == "ok"
    assert payload["created_tables"] == CORE_TABLES
    assert payload["already_present"] == []
    assert payload["missing"] == []
    assert await _list_public_tables(sample_dsn) == CORE_TABLES


@pytest.mark.asyncio
async def test_init_db_command_reports_existing_tables(sample_dsn: str) -> None:
    result = _run_init_db(sample_dsn)

    assert result.returncode == 0
    payload = json.loads(result.stdout)
    assert payload["status"] == "ok"
    assert payload["created_tables"] == []
    assert payload["already_present"] == CORE_TABLES
    assert payload["missing"] == []
