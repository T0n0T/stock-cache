from pathlib import Path

import pytest

from stock_cache.config import Settings
from stock_cache.db.pool import create_pool
from stock_cache.repositories.market_data import MarketDataRepository
from stock_cache.use_cases.write_market_data import WriteMarketDataUseCase


class LiveWriteProvider:
    def fetch_instruments(self) -> list[object]:
        class InstrumentRecord:
            ts_code = "LIVEWR1.SZ"

        return [InstrumentRecord()]

    def fetch_recent_trade_dates(self, end_date: str, limit: int) -> list[str]:
        _ = end_date
        return ["20260331", "20260330"][:limit]

    def fetch_daily(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        _ = (start_date, end_date)
        return [
            {
                "ts_code": ts_code,
                "trade_date": "20260331",
                "open": 10.0,
                "high": 10.8,
                "low": 9.9,
                "close": 10.5,
                "pct_chg": 1.5,
            }
        ]

    def fetch_daily_basic(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        _ = (ts_code, start_date, end_date)
        return [{"trade_date": "20260331", "turnover_rate": 2.5, "total_mv": 123.4}]

    def fetch_moneyflow(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        _ = (ts_code, start_date, end_date)
        return [{"trade_date": "20260331", "net_mf_amount": 5.6}]

    def fetch_indicators(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        _ = (ts_code, start_date, end_date)
        return [
            {
                "trade_date": "20260331",
                "macd": 0.1,
                "macd_dif": 0.2,
                "macd_dea": 0.3,
                "kdj_k": 40.0,
                "kdj_d": 41.0,
                "kdj_j": 42.0,
            }
        ]


@pytest.mark.asyncio
async def test_write_use_case_persists_market_and_indicator_rows_to_live_postgres(
    sample_dsn: str, tmp_path: Path
) -> None:
    pool = await create_pool(sample_dsn)
    repository = MarketDataRepository(pool)
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
        fallback_provider=provider,
        market_repository=repository,
        instrument_repository=None,
        job_run_repository=None,
    )

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
    finally:
        async with pool.acquire() as connection:
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
