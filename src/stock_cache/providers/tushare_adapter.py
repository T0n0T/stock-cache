from collections.abc import Sequence

import tushare as ts

from stock_cache.domain.models import Instrument


class TushareAdapter:
    def __init__(self, token: str) -> None:
        self._pro = ts.pro_api(token)

    def fetch_instruments(self) -> Sequence[Instrument]:
        raise NotImplementedError("TushareAdapter.fetch_instruments is implemented in Task 10")

    def fetch_recent_trade_dates(self, end_date: str, limit: int) -> Sequence[str]:
        raise NotImplementedError("TushareAdapter.fetch_recent_trade_dates is implemented in Task 10")

    def fetch_daily(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        frame = self._pro.daily(ts_code=ts_code, start_date=start_date, end_date=end_date)
        return frame.to_dict("records")

    def fetch_daily_basic(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        raise NotImplementedError("TushareAdapter.fetch_daily_basic is implemented in Task 10")

    def fetch_moneyflow(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        raise NotImplementedError("TushareAdapter.fetch_moneyflow is implemented in Task 10")

    def fetch_indicators(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        raise NotImplementedError("TushareAdapter.fetch_indicators is implemented in Task 10")
