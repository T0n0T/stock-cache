from collections.abc import Sequence

from stock_cache.domain.models import Instrument


class AkshareAdapter:
    def fetch_instruments(self) -> Sequence[Instrument]:
        raise NotImplementedError("AkshareAdapter.fetch_instruments is implemented in Task 10")

    def fetch_recent_trade_dates(self, end_date: str, limit: int) -> Sequence[str]:
        raise NotImplementedError("AkshareAdapter.fetch_recent_trade_dates is implemented in Task 10")

    def fetch_daily(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        raise NotImplementedError("AkshareAdapter.fetch_daily is implemented in Task 10")

    def fetch_daily_basic(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        raise NotImplementedError("AkshareAdapter.fetch_daily_basic is implemented in Task 10")

    def fetch_moneyflow(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        raise NotImplementedError("AkshareAdapter.fetch_moneyflow is implemented in Task 10")

    def fetch_indicators(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        raise NotImplementedError("AkshareAdapter.fetch_indicators is implemented in Task 10")
