from collections.abc import Sequence
from typing import Protocol

from stock_cache.domain.models import Instrument


class MarketDataProvider(Protocol):
    def fetch_instruments(self) -> Sequence[Instrument]:
        raise NotImplementedError

    def fetch_recent_trade_dates(self, end_date: str, limit: int) -> Sequence[str]:
        raise NotImplementedError

    def fetch_daily(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        raise NotImplementedError

    def fetch_daily_basic(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        raise NotImplementedError

    def fetch_moneyflow(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        raise NotImplementedError

    def fetch_indicators(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        raise NotImplementedError
