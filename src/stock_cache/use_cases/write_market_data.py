from dataclasses import dataclass
from datetime import UTC, datetime
from typing import Callable

from stock_cache.config import Settings
from stock_cache.domain.errors import StockCacheError
from stock_cache.domain.models import JobRunSummary
from stock_cache.services.normalizer import normalize_symbol_bundle
from stock_cache.services.retry import with_retries
from stock_cache.services.status_reporter import StatusReporter


@dataclass(slots=True)
class WriteMarketDataUseCase:
    settings: Settings
    primary_provider: object
    fallback_provider: object
    market_repository: object | None
    instrument_repository: object | None
    job_run_repository: object | None
    now_provider: Callable[[], datetime] = lambda: datetime.now(UTC)

    async def run(self, mode: str, symbols: list[str] | None = None) -> JobRunSummary:
        _ = mode
        reporter = StatusReporter(self.settings.status_file_path)
        if symbols is None:
            instruments = list(self.primary_provider.fetch_instruments())
            target_symbols = [instrument.ts_code for instrument in instruments]
        else:
            target_symbols = symbols
        date_window = self._date_window() if target_symbols else None
        successes: list[str] = []
        failures: dict[str, str] = {}
        started_at = self.now_provider().isoformat()

        for ts_code in target_symbols:
            try:
                await with_retries(
                    lambda ts_code=ts_code, date_window=date_window: self._process_symbol(ts_code, date_window),
                    max_retries=self.settings.max_retries,
                    base_delay=self.settings.retry_base_delay,
                    backoff_factor=self.settings.retry_backoff_factor,
                    jitter=self.settings.retry_jitter,
                )
                successes.append(ts_code)
            except StockCacheError as exc:
                failures[ts_code] = str(exc)

        summary = JobRunSummary(
            job_id=self.now_provider().strftime("%Y%m%dT%H%M%SZ"),
            status="success" if not failures else "partial_success",
            started_at=started_at,
            finished_at=self.now_provider().isoformat(),
            total_symbols=len(target_symbols),
            success_symbols=successes,
            failed_symbols=failures,
        )
        reporter.write(summary)
        return summary

    async def _process_symbol(self, ts_code: str, date_window: tuple[str, str] | None) -> None:
        if date_window is None:
            raise ValueError("date_window is required when processing symbols")
        start_date, end_date = date_window
        daily_rows = self.primary_provider.fetch_daily(ts_code, start_date, end_date)
        daily_basic_rows = self.primary_provider.fetch_daily_basic(ts_code, start_date, end_date)
        moneyflow_rows = self.primary_provider.fetch_moneyflow(ts_code, start_date, end_date)
        indicator_rows = self.primary_provider.fetch_indicators(ts_code, start_date, end_date)
        bundle = normalize_symbol_bundle(
            ts_code,
            daily_rows,
            daily_basic_rows,
            moneyflow_rows,
            indicator_rows,
        )
        _ = bundle

    def _date_window(self) -> tuple[str, str]:
        end_date = self.now_provider().strftime("%Y%m%d")
        trade_dates = list(
            self.primary_provider.fetch_recent_trade_dates(
                end_date,
                self.settings.default_lookback_trading_days,
            )
        )
        if not trade_dates:
            raise ValueError("provider returned no trade dates for configured lookback window")
        return min(trade_dates), max(trade_dates)
