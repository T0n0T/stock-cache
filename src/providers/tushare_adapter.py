import asyncio
from datetime import datetime, timedelta
from collections.abc import Sequence
from queue import Empty, Queue
from threading import Thread

from requests import RequestException
import tushare as ts

from domain.errors import RetryableProviderError
from domain.models import Instrument


def _infer_exchange(ts_code: str) -> str:
    if ts_code.endswith(".SH"):
        return "SSE"
    if ts_code.endswith(".SZ"):
        return "SZSE"
    if ts_code.endswith(".BJ"):
        return "BSE"
    return ""


class TushareAdapter:
    def __init__(self, token: str, timeout_seconds: float = 20) -> None:
        self._pro = ts.pro_api(token)
        self._timeout_seconds = timeout_seconds

    def fetch_instruments(self) -> Sequence[Instrument]:
        frame = self._safe_query(self._pro.stock_basic, list_status="L")
        return [
            Instrument(
                ts_code=str(row["ts_code"]),
                symbol=str(row.get("symbol") or str(row["ts_code"]).split(".")[0]),
                name=str(row.get("name") or row.get("fullname") or row["ts_code"]),
                exchange=row.get("exchange") or _infer_exchange(str(row["ts_code"])),
                list_status=str(row.get("list_status") or "L"),
                is_st=False,
            )
            for row in frame.to_dict("records")
        ]

    def fetch_recent_trade_dates(self, end_date: str, limit: int) -> Sequence[str]:
        end = datetime.strptime(end_date, "%Y%m%d").date()
        start = (end - timedelta(days=max(limit * 4 - 1, 0))).strftime("%Y%m%d")
        frame = self._safe_query(self._pro.trade_cal, exchange="SSE", start_date=start, end_date=end_date)
        rows = frame.to_dict("records")
        open_dates = [str(row["cal_date"]) for row in rows if int(row.get("is_open", 0)) == 1]
        return open_dates[:limit]

    def fetch_trade_dates_in_range(self, start_date: str, end_date: str) -> Sequence[str]:
        frame = self._safe_query(self._pro.trade_cal, exchange="SSE", start_date=start_date, end_date=end_date)
        rows = frame.to_dict("records")
        return [str(row["cal_date"]) for row in rows if int(row.get("is_open", 0)) == 1]

    def fetch_daily(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        frame = self._safe_query(self._pro.daily, ts_code=ts_code, start_date=start_date, end_date=end_date)
        return [
            {
                **row,
                "source_daily": "daily",
            }
            for row in frame.to_dict("records")
        ]

    def fetch_daily_basic(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        frame = self._safe_query(self._pro.daily_basic, ts_code=ts_code, start_date=start_date, end_date=end_date)
        return [
            {
                **row,
                "source_daily_basic": "daily_basic",
            }
            for row in frame.to_dict("records")
        ]

    def fetch_moneyflow(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        frame = self._safe_query(self._pro.moneyflow, ts_code=ts_code, start_date=start_date, end_date=end_date)
        return [
            {
                **row,
                "source_moneyflow": "moneyflow",
            }
            for row in frame.to_dict("records")
        ]

    def fetch_indicators(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        frame = self._safe_query(self._pro.stk_factor, ts_code=ts_code, start_date=start_date, end_date=end_date)
        return frame.to_dict("records")

    async def fetch_daily_by_trade_date(self, trade_date: str) -> list[dict[str, object]]:
        frame = await self._safe_query_async(self._pro.daily, trade_date=trade_date)
        return [{**row, "source_daily": "daily"} for row in frame.to_dict("records")]

    async def fetch_daily_basic_by_trade_date(self, trade_date: str) -> list[dict[str, object]]:
        frame = await self._safe_query_async(self._pro.daily_basic, trade_date=trade_date)
        return [{**row, "source_daily_basic": "daily_basic"} for row in frame.to_dict("records")]

    async def fetch_moneyflow_by_trade_date(self, trade_date: str) -> list[dict[str, object]]:
        frame = await self._safe_query_async(self._pro.moneyflow, trade_date=trade_date)
        return [{**row, "source_moneyflow": "moneyflow"} for row in frame.to_dict("records")]

    async def fetch_adj_factor_by_trade_date(self, trade_date: str) -> list[dict[str, object]]:
        frame = await self._safe_query_async(self._pro.adj_factor, trade_date=trade_date)
        return frame.to_dict("records")

    async def fetch_stk_limit_by_trade_date(self, trade_date: str) -> list[dict[str, object]]:
        frame = await self._safe_query_async(self._pro.stk_limit, trade_date=trade_date)
        return frame.to_dict("records")

    async def fetch_suspend_d_by_trade_date(self, trade_date: str) -> list[dict[str, object]]:
        frame = await self._safe_query_async(self._pro.suspend_d, trade_date=trade_date)
        return frame.to_dict("records")

    async def fetch_indicators_by_trade_date(self, trade_date: str) -> list[dict[str, object]]:
        try:
            frame = await self._safe_query_async(self._pro.stk_factor_pro, trade_date=trade_date)
            source_interface = "stk_factor_pro"
        except Exception as exc:
            if not self._is_permission_error(exc):
                raise
            frame = await self._safe_query_async(self._pro.stk_factor, trade_date=trade_date)
            source_interface = "stk_factor"
        return [{**row, "source_interface": source_interface} for row in frame.to_dict("records")]

    async def _safe_query_async(self, method: object, **kwargs: object) -> object:
        return await asyncio.to_thread(self._safe_query, method, **kwargs)

    def _safe_query(self, method: object, **kwargs: object) -> object:
        result_queue: Queue[tuple[str, object]] = Queue(maxsize=1)

        def runner() -> None:
            try:
                result_queue.put(("ok", method(**kwargs)))
            except Exception as exc:  # pragma: no cover - covered via consumer assertions
                result_queue.put(("error", exc))

        thread = Thread(target=runner, daemon=True)
        thread.start()
        try:
            status, payload = result_queue.get(timeout=self._timeout_seconds)
        except Empty as exc:
            raise RetryableProviderError(f"provider query timed out after {self._timeout_seconds} seconds") from exc

        if status == "error":
            if isinstance(payload, RequestException):
                raise RetryableProviderError(str(payload)) from payload
            raise payload
        return payload

    @staticmethod
    def _is_permission_error(exc: Exception) -> bool:
        text = str(exc).lower()
        return "权限" in str(exc) or "permission" in text or "没有访问该接口" in str(exc)
