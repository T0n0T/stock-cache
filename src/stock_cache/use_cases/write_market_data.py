from dataclasses import dataclass
from datetime import UTC, datetime

from stock_cache.config import Settings
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

    async def run(self, mode: str, symbols: list[str] | None = None) -> JobRunSummary:
        _ = mode
        reporter = StatusReporter(self.settings.status_file_path)
        instruments = list(self.primary_provider.fetch_instruments())
        target_symbols = symbols or [instrument.ts_code for instrument in instruments]
        successes: list[str] = []
        failures: dict[str, str] = {}
        started_at = datetime.now(UTC).isoformat()

        for ts_code in target_symbols:
            try:
                await with_retries(
                    lambda ts_code=ts_code: self._process_symbol(ts_code),
                    max_retries=self.settings.max_retries,
                    base_delay=self.settings.retry_base_delay,
                    backoff_factor=self.settings.retry_backoff_factor,
                    jitter=self.settings.retry_jitter,
                )
                successes.append(ts_code)
            except Exception as exc:
                failures[ts_code] = str(exc)

        summary = JobRunSummary(
            job_id=datetime.now(UTC).strftime("%Y%m%dT%H%M%SZ"),
            status="success" if not failures else "partial_success",
            started_at=started_at,
            finished_at=datetime.now(UTC).isoformat(),
            total_symbols=len(target_symbols),
            success_symbols=successes,
            failed_symbols=failures,
        )
        reporter.write(summary)
        return summary

    async def _process_symbol(self, ts_code: str) -> None:
        daily_rows = self.primary_provider.fetch_daily(ts_code, "20260101", "20260330")
        daily_basic_rows = self.primary_provider.fetch_daily_basic(ts_code, "20260101", "20260330")
        moneyflow_rows = self.primary_provider.fetch_moneyflow(ts_code, "20260101", "20260330")
        indicator_rows = self.primary_provider.fetch_indicators(ts_code, "20260101", "20260330")
        bundle = normalize_symbol_bundle(
            ts_code,
            daily_rows,
            daily_basic_rows,
            moneyflow_rows,
            indicator_rows,
        )
        _ = bundle
