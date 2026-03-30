from pathlib import Path

import pytest

from stock_cache.config import Settings
from stock_cache.domain.errors import RetryableProviderError
from stock_cache.domain.models import Instrument
from stock_cache.use_cases.write_market_data import WriteMarketDataUseCase


class FlakyProvider:
    def __init__(self) -> None:
        self.calls = 0

    def fetch_instruments(self) -> list[Instrument]:
        return [
            Instrument(
                ts_code="000001.SZ",
                symbol="000001",
                name="Ping An",
                exchange="SZ",
                list_status="L",
                is_st=False,
            )
        ]

    def fetch_daily(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        self.calls += 1
        if self.calls == 1:
            raise RetryableProviderError("timeout")
        return [{"trade_date": "20260330", "close": 12.3, "pct_chg": 1.1}]

    def fetch_daily_basic(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        return [{"trade_date": "20260330", "turnover_rate": 1.2, "total_mv": 1000.0}]

    def fetch_moneyflow(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        return [{"trade_date": "20260330", "net_mf_amount": 12.4}]

    def fetch_indicators(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        return [{"trade_date": "20260330", "macd": 0.1, "kdj_j": 70.0}]


@pytest.mark.asyncio
async def test_write_use_case_retries_per_symbol_and_writes_status(tmp_path: Path) -> None:
    provider = FlakyProvider()
    status_file = tmp_path / "last-write-status.txt"
    use_case = WriteMarketDataUseCase(
        settings=Settings(
            POSTGRES_DSN="postgresql://postgres:postgres@localhost:5432/stock_cache",
            TUSHARE_TOKEN="token",
            STATUS_FILE_PATH=status_file,
        ),
        primary_provider=provider,
        fallback_provider=provider,
        market_repository=None,
        instrument_repository=None,
        job_run_repository=None,
    )

    summary = await use_case.run(mode="full")

    assert provider.calls == 2
    assert summary.success_symbols == ["000001.SZ"]
    assert status_file.exists()
