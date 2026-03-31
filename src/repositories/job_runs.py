from datetime import datetime

import asyncpg

from domain.models import JobRunSummary


class JobRunRepository:
    def __init__(self, pool: asyncpg.Pool) -> None:
        self._pool = pool

    async def insert_job_run(self, summary: JobRunSummary, status_file_path: str, job_type: str = "write") -> None:
        sql = """
        INSERT INTO job_runs (
            job_id,
            job_type,
            started_at,
            finished_at,
            status,
            total_symbols,
            success_symbols,
            failed_symbols,
            status_file_path
        )
        VALUES ($1, $2, $3::timestamptz, $4::timestamptz, $5, $6, $7, $8, $9)
        ON CONFLICT (job_id) DO UPDATE
        SET job_type = EXCLUDED.job_type,
            started_at = EXCLUDED.started_at,
            finished_at = EXCLUDED.finished_at,
            status = EXCLUDED.status,
            total_symbols = EXCLUDED.total_symbols,
            success_symbols = EXCLUDED.success_symbols,
            failed_symbols = EXCLUDED.failed_symbols,
            status_file_path = EXCLUDED.status_file_path
        """
        async with self._pool.acquire() as connection:
            await connection.execute(
                sql,
                summary.job_id,
                job_type,
                datetime.fromisoformat(summary.started_at),
                datetime.fromisoformat(summary.finished_at),
                summary.status,
                summary.total_symbols,
                len(summary.success_symbols),
                len(summary.failed_symbols),
                status_file_path,
            )
