import asyncio
from dataclasses import dataclass
from datetime import UTC, datetime
from typing import Callable

from config import Settings
from domain.errors import StockCacheError
from domain.models import JobRunSummary
from services.normalizer import normalize_market_batches
from services.retry import with_retries
from services.status_reporter import StatusReporter


@dataclass(slots=True)
class WriteMarketDataUseCase:
    settings: Settings
    primary_provider: object
    market_repository: object | None
    instrument_repository: object | None
    job_run_repository: object | None
    now_provider: Callable[[], datetime] = lambda: datetime.now(UTC)

    async def run(self, mode: str, symbols: list[str] | None = None) -> JobRunSummary:
        _ = mode
        reporter = StatusReporter(self.settings.status_file_path)
        started_at = self.now_provider().isoformat()
        try:
            instruments = []
            if symbols is None:
                instruments = list(self.primary_provider.fetch_instruments())
                target_symbols = [instrument.ts_code for instrument in instruments]
            else:
                target_symbols = symbols
            if self.instrument_repository is not None and instruments:
                await self.instrument_repository.upsert_instruments(instruments)
            trade_dates = self._trade_dates() if target_symbols else []
        except Exception as exc:
            summary = JobRunSummary(
                job_id=self.now_provider().strftime("%Y%m%dT%H%M%SZ"),
                status="failed",
                started_at=started_at,
                finished_at=self.now_provider().isoformat(),
                total_symbols=0,
                success_symbols=[],
                failed_symbols={"__startup__": str(exc)},
            )
            reporter.write(summary)
            if self.job_run_repository is not None:
                await self.job_run_repository.insert_job_run(
                    summary,
                    status_file_path=str(self.settings.status_file_path),
                )
            return summary

        successes: list[str] = []
        failures: dict[str, str] = {}
        daily_rows: list[dict[str, object]] = []
        daily_basic_rows: list[dict[str, object]] = []
        moneyflow_rows: list[dict[str, object]] = []
        adj_factor_rows: list[dict[str, object]] = []
        limit_rows: list[dict[str, object]] = []
        suspend_rows: list[dict[str, object]] = []
        indicator_rows: list[dict[str, object]] = []
        for trade_date in trade_dates:
            try:
                payload = await with_retries(
                    lambda trade_date=trade_date: self._fetch_trade_date_payload(trade_date),
                    max_retries=self.settings.max_retries,
                    base_delay=self.settings.retry_base_delay,
                    backoff_factor=self.settings.retry_backoff_factor,
                    jitter=self.settings.retry_jitter,
                )
                daily_rows.extend(payload[0])
                daily_basic_rows.extend(payload[1])
                moneyflow_rows.extend(payload[2])
                adj_factor_rows.extend(payload[3])
                limit_rows.extend(payload[4])
                suspend_rows.extend(payload[5])
                indicator_rows.extend(payload[6])
            except StockCacheError as exc:
                failures[f"__trade_date__:{trade_date}"] = str(exc)
            except Exception as exc:
                failures[f"__trade_date__:{trade_date}"] = str(exc)

        if target_symbols:
            bundle = normalize_market_batches(
                daily_rows=daily_rows,
                daily_basic_rows=daily_basic_rows,
                moneyflow_rows=moneyflow_rows,
                adj_factor_rows=adj_factor_rows,
                limit_rows=limit_rows,
                suspend_rows=suspend_rows,
                indicator_rows=indicator_rows,
                target_symbols=set(target_symbols),
            )
            if self.market_repository is not None:
                await self.market_repository.upsert_daily_market(bundle.market_rows)
                await self.market_repository.upsert_daily_indicators(bundle.indicator_rows)
        if target_symbols and not failures:
            successes = list(target_symbols)

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
        if self.job_run_repository is not None:
            await self.job_run_repository.insert_job_run(
                summary,
                status_file_path=str(self.settings.status_file_path),
            )
        return summary

    async def _fetch_trade_date_payload(
        self,
        trade_date: str,
    ) -> tuple[
        list[dict[str, object]],
        list[dict[str, object]],
        list[dict[str, object]],
        list[dict[str, object]],
        list[dict[str, object]],
        list[dict[str, object]],
        list[dict[str, object]],
    ]:
        return await self._fetch_trade_date_payload_for_provider(self.primary_provider, trade_date)

    @staticmethod
    async def _fetch_trade_date_payload_for_provider(
        provider: object,
        trade_date: str,
    ) -> tuple[
        list[dict[str, object]],
        list[dict[str, object]],
        list[dict[str, object]],
        list[dict[str, object]],
        list[dict[str, object]],
        list[dict[str, object]],
        list[dict[str, object]],
    ]:
        (
            daily_rows,
            daily_basic_rows,
            moneyflow_rows,
            adj_factor_rows,
            limit_rows,
            suspend_rows,
            indicator_rows,
        ) = await asyncio.gather(
            provider.fetch_daily_by_trade_date(trade_date),
            provider.fetch_daily_basic_by_trade_date(trade_date),
            provider.fetch_moneyflow_by_trade_date(trade_date),
            provider.fetch_adj_factor_by_trade_date(trade_date),
            provider.fetch_stk_limit_by_trade_date(trade_date),
            provider.fetch_suspend_d_by_trade_date(trade_date),
            provider.fetch_indicators_by_trade_date(trade_date),
        )
        return (
            daily_rows,
            daily_basic_rows,
            moneyflow_rows,
            adj_factor_rows,
            limit_rows,
            suspend_rows,
            indicator_rows,
        )

    def _trade_dates(self) -> list[str]:
        end_date = self.now_provider().strftime("%Y%m%d")
        trade_dates = list(
            self.primary_provider.fetch_recent_trade_dates(
                end_date,
                self.settings.default_lookback_trading_days,
            )
        )
        if not trade_dates:
            raise ValueError("provider returned no trade dates for configured lookback window")
        return trade_dates
