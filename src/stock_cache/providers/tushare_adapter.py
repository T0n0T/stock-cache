from datetime import datetime, timedelta
from collections.abc import Sequence

import tushare as ts

from stock_cache.domain.models import Instrument


class TushareAdapter:
    def __init__(self, token: str) -> None:
        self._pro = ts.pro_api(token)

    def fetch_instruments(self) -> Sequence[Instrument]:
        frame = self._pro.stock_basic(list_status="L")
        return [
            Instrument(
                ts_code=row["ts_code"],
                symbol=row["symbol"],
                name=row["name"],
                exchange=row["exchange"],
                list_status=row["list_status"],
                is_st=False,
            )
            for row in frame.to_dict("records")
        ]

    def fetch_recent_trade_dates(self, end_date: str, limit: int) -> Sequence[str]:
        end = datetime.strptime(end_date, "%Y%m%d").date()
        start = (end - timedelta(days=max(limit * 4 - 1, 0))).strftime("%Y%m%d")
        frame = self._pro.trade_cal(exchange="SSE", start_date=start, end_date=end_date)
        rows = frame.to_dict("records")
        open_dates = [str(row["cal_date"]) for row in rows if int(row.get("is_open", 0)) == 1]
        return open_dates[:limit]

    def fetch_daily(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        frame = self._pro.daily(ts_code=ts_code, start_date=start_date, end_date=end_date)
        return frame.to_dict("records")

    def fetch_daily_basic(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        frame = self._pro.daily_basic(ts_code=ts_code, start_date=start_date, end_date=end_date)
        return frame.to_dict("records")

    def fetch_moneyflow(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        frame = self._pro.moneyflow(ts_code=ts_code, start_date=start_date, end_date=end_date)
        return frame.to_dict("records")

    def fetch_indicators(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        frame = self._pro.stk_factor(ts_code=ts_code, start_date=start_date, end_date=end_date)
        return frame.to_dict("records")
