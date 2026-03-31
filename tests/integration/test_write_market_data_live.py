from pathlib import Path

import pytest

from config import Settings
from db.pool import create_pool
from domain.models import Instrument
from repositories.instruments import InstrumentRepository
from repositories.job_runs import JobRunRepository
from repositories.market_data import MarketDataRepository
from use_cases.write_market_data import WriteMarketDataUseCase


class LiveWriteProvider:
    def fetch_instruments(self) -> list[Instrument]:
        return [
            Instrument(
                ts_code="LIVEWR1.SZ",
                symbol="LIVEWR1",
                name="Live Write Instrument",
                exchange="SZ",
                list_status="L",
                is_st=False,
            )
        ]

    def fetch_recent_trade_dates(self, end_date: str, limit: int) -> list[str]:
        _ = end_date
        return ["20260331", "20260330"][:limit]

    async def fetch_daily_by_trade_date(self, trade_date: str) -> list[dict[str, object]]:
        if trade_date != "20260331":
            return []
        return [
            {
                "ts_code": "LIVEWR1.SZ",
                "trade_date": "20260331",
                "open": 10.0,
                "high": 10.8,
                "low": 9.9,
                "close": 10.5,
                "pct_chg": 1.5,
            }
        ]

    async def fetch_daily_basic_by_trade_date(self, trade_date: str) -> list[dict[str, object]]:
        if trade_date != "20260331":
            return []
        return [{"ts_code": "LIVEWR1.SZ", "trade_date": "20260331", "turnover_rate": 2.5, "total_mv": 123.4}]

    async def fetch_moneyflow_by_trade_date(self, trade_date: str) -> list[dict[str, object]]:
        if trade_date != "20260331":
            return []
        return [{"ts_code": "LIVEWR1.SZ", "trade_date": "20260331", "net_mf_amount": 5.6}]

    async def fetch_adj_factor_by_trade_date(self, trade_date: str) -> list[dict[str, object]]:
        _ = trade_date
        return []

    async def fetch_stk_limit_by_trade_date(self, trade_date: str) -> list[dict[str, object]]:
        _ = trade_date
        return []

    async def fetch_suspend_d_by_trade_date(self, trade_date: str) -> list[dict[str, object]]:
        _ = trade_date
        return []

    async def fetch_indicators_by_trade_date(self, trade_date: str) -> list[dict[str, object]]:
        if trade_date != "20260331":
            return []
        return [
            {
                "ts_code": "LIVEWR1.SZ",
                "trade_date": "20260331",
                "macd": 0.1,
                "macd_dif": 0.2,
                "macd_dea": 0.3,
                "kdj_k": 40.0,
                "kdj_d": 41.0,
                "kdj_j": 42.0,
                "source_interface": "integration-test",
            }
        ]


@pytest.mark.asyncio
async def test_write_use_case_persists_market_and_indicator_rows_to_live_postgres(
    sample_dsn: str, tmp_path: Path
) -> None:
    pool = await create_pool(sample_dsn)
    repository = MarketDataRepository(pool)
    instrument_repository = InstrumentRepository(pool)
    job_run_repository = JobRunRepository(pool)
    provider = LiveWriteProvider()
    status_file = tmp_path / "last-write-status.txt"
    use_case = WriteMarketDataUseCase(
        settings=Settings(
            POSTGRES_DSN=sample_dsn,
            TUSHARE_TOKEN="token",
            STATUS_FILE_PATH=status_file,
            DEFAULT_LOOKBACK_TRADING_DAYS=2,
        ),
        primary_provider=provider,
        market_repository=repository,
        instrument_repository=instrument_repository,
        job_run_repository=job_run_repository,
    )
    summary = None

    try:
        summary = await use_case.run(mode="full")

        async with pool.acquire() as connection:
            market = await connection.fetchrow(
                "select close, turnover_rate, total_mv, net_mf_amount from daily_market where ts_code = $1 and trade_date = date '2026-03-31'",
                "LIVEWR1.SZ",
            )
            indicator = await connection.fetchrow(
                "select macd, macd_dif, macd_dea, kdj_k, kdj_d, kdj_j from daily_indicators where ts_code = $1 and trade_date = date '2026-03-31'",
                "LIVEWR1.SZ",
            )
            instrument = await connection.fetchrow(
                "select symbol, name, exchange, list_status, is_st from instruments where ts_code = $1",
                "LIVEWR1.SZ",
            )
            job_run = await connection.fetchrow(
                """
                select job_id, status, total_symbols, success_symbols, failed_symbols, status_file_path
                from job_runs
                where job_id = $1
                """,
                summary.job_id,
            )
    finally:
        async with pool.acquire() as connection:
            if summary is not None:
                await connection.execute("delete from job_runs where job_id = $1", summary.job_id)
            await connection.execute("delete from instruments where ts_code = $1", "LIVEWR1.SZ")
            await connection.execute("delete from daily_indicators where ts_code = $1", "LIVEWR1.SZ")
            await connection.execute("delete from daily_market where ts_code = $1", "LIVEWR1.SZ")
        await pool.close()

    assert summary.success_symbols == ["LIVEWR1.SZ"]
    assert status_file.exists()
    assert market is not None
    assert market["close"] == 10.5
    assert market["turnover_rate"] == 2.5
    assert market["total_mv"] == 123.4
    assert market["net_mf_amount"] == 5.6
    assert indicator is not None
    assert indicator["macd"] == 0.1
    assert indicator["macd_dif"] == 0.2
    assert indicator["macd_dea"] == 0.3
    assert indicator["kdj_k"] == 40.0
    assert indicator["kdj_d"] == 41.0
    assert indicator["kdj_j"] == 42.0
    assert instrument is not None
    assert instrument["symbol"] == "LIVEWR1"
    assert instrument["name"] == "Live Write Instrument"
    assert instrument["exchange"] == "SZ"
    assert instrument["list_status"] == "L"
    assert instrument["is_st"] is False
    assert job_run is not None
    assert job_run["status"] == "success"
    assert job_run["total_symbols"] == 1
    assert job_run["success_symbols"] == 1
    assert job_run["failed_symbols"] == 0
    assert job_run["status_file_path"] == str(status_file)
