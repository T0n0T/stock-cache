from datetime import date

import pytest

from stock_cache.domain.errors import RetryableProviderError
from stock_cache.domain.models import DailyMarketRow
from stock_cache.services.indicators import calculate_macd_fallback
from stock_cache.services.retry import with_retries


def test_calculate_macd_fallback_returns_rows_for_market_series() -> None:
    rows = [
        DailyMarketRow(ts_code="000001.SZ", trade_date=date(2026, 3, day), close=float(day))
        for day in range(1, 31)
    ]

    indicators = calculate_macd_fallback(rows)

    assert len(indicators) == 30
    assert indicators[0].ts_code == "000001.SZ"
    assert indicators[0].trade_date == date(2026, 3, 1)
    assert indicators[0].calc_fallback_used is True
    assert indicators[0].source_provider == "local"
    assert indicators[0].source_interface == "macd_fallback"
    assert indicators[-1].macd is not None
    assert indicators[-1].ts_code == "000001.SZ"
    assert indicators[-1].trade_date == date(2026, 3, 30)
    assert indicators[-1].calc_fallback_used is True
    assert indicators[-1].source_provider == "local"
    assert indicators[-1].source_interface == "macd_fallback"


def test_calculate_macd_fallback_rejects_missing_close() -> None:
    rows = [DailyMarketRow(ts_code="000001.SZ", trade_date=date(2026, 3, 1), close=None)]

    with pytest.raises(ValueError, match="close price is required"):
        calculate_macd_fallback(rows)


@pytest.mark.asyncio
async def test_with_retries_retries_retryable_operation_until_success() -> None:
    attempts = 0

    async def operation() -> str:
        nonlocal attempts
        attempts += 1
        if attempts < 3:
            raise RetryableProviderError("temporary")
        return "ok"

    result = await with_retries(
        operation=operation,
        max_retries=3,
        base_delay=0.0,
        backoff_factor=2.0,
        jitter=0.0,
    )

    assert result == "ok"
    assert attempts == 3


@pytest.mark.asyncio
async def test_with_retries_reraises_after_retry_budget_exhausted() -> None:
    attempts = 0

    async def operation() -> str:
        nonlocal attempts
        attempts += 1
        raise RetryableProviderError("still failing")

    with pytest.raises(RetryableProviderError, match="still failing"):
        await with_retries(
            operation=operation,
            max_retries=2,
            base_delay=0.0,
            backoff_factor=2.0,
            jitter=0.0,
        )

    assert attempts == 3
