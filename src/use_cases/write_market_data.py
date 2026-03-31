import asyncio
from dataclasses import dataclass
from datetime import UTC, datetime
from typing import Callable

from config import Settings
from domain.errors import StockCacheError
from domain.models import JobRunSummary
from services.normalizer import normalize_market_batches, normalize_symbol_bundle
from services.retry import with_retries
from services.status_reporter import StatusReporter


@dataclass(slots=True)
class WriteDateRange:
    lookback_trading_days: int | None = None
    start_date: str | None = None
    end_date: str | None = None


@dataclass(slots=True)
class WriteMarketDataUseCase:
    settings: Settings
    primary_provider: object
    market_repository: object | None
    instrument_repository: object | None
    job_run_repository: object | None
    now_provider: Callable[[], datetime] = lambda: datetime.now(UTC)

    async def run(
        self,
        mode: str,
        symbols: list[str] | None = None,
        write_range: WriteDateRange | None = None,
        progress: Callable[[str], None] | None = None,
    ) -> JobRunSummary:
        if progress is None:
            progress = lambda message: None
        reporter = StatusReporter(self.settings.status_file_path)
        started_at = self.now_provider().isoformat()
        try:
            self._emit_progress(progress, f"write started: mode={mode}")
            instruments = []
            if symbols is None:
                self._emit_progress(progress, "loading active instrument universe")
                instruments = list(self.primary_provider.fetch_instruments())
                target_symbols = [instrument.ts_code for instrument in instruments]
                self._emit_progress(progress, f"loaded {len(target_symbols)} active instruments")
            else:
                target_symbols = symbols
                self._emit_progress(progress, f"selected {len(target_symbols)} target symbol(s): {', '.join(target_symbols)}")
            if self.instrument_repository is not None and instruments:
                await self.instrument_repository.upsert_instruments(instruments)
            trade_dates = self._trade_dates(write_range=write_range) if target_symbols else []
            if trade_dates:
                self._emit_progress(
                    progress,
                    f"resolved {len(trade_dates)} trade date(s): {min(trade_dates)} -> {max(trade_dates)}",
                )
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

        if mode == "single":
            return await self._run_single_symbol_write(
                target_symbols=target_symbols,
                trade_dates=trade_dates,
                started_at=started_at,
                reporter=reporter,
                progress=progress,
            )
        if mode != "full":
            raise ValueError(f"unsupported write mode: {mode}")

        successes: list[str] = []
        failures: dict[str, str] = {}
        target_symbol_set = set(target_symbols)
        for index, trade_date in enumerate(trade_dates, start=1):
            self._emit_progress(progress, f"syncing trade date {trade_date} ({index}/{len(trade_dates)})")
            try:
                payload = await with_retries(
                    lambda trade_date=trade_date: self._fetch_trade_date_payload(trade_date),
                    max_retries=self.settings.max_retries,
                    base_delay=self.settings.retry_base_delay,
                    backoff_factor=self.settings.retry_backoff_factor,
                    jitter=self.settings.retry_jitter,
                )
                bundle = normalize_market_batches(
                    daily_rows=payload[0],
                    daily_basic_rows=payload[1],
                    moneyflow_rows=payload[2],
                    adj_factor_rows=payload[3],
                    limit_rows=payload[4],
                    suspend_rows=payload[5],
                    indicator_rows=payload[6],
                    target_symbols=target_symbol_set,
                )
                if self.market_repository is not None:
                    self._emit_progress(
                        progress,
                        f"persisting {len(bundle.market_rows)} market row(s) and {len(bundle.indicator_rows)} indicator row(s)",
                    )
                    await self.market_repository.upsert_daily_market(bundle.market_rows)
                    await self.market_repository.upsert_daily_indicators(bundle.indicator_rows)
            except StockCacheError as exc:
                failures[f"__trade_date__:{trade_date}"] = str(exc)
                self._emit_progress(progress, f"trade date {trade_date} failed: {exc}")
            except Exception as exc:
                failures[f"__trade_date__:{trade_date}"] = str(exc)
                self._emit_progress(progress, f"trade date {trade_date} failed: {exc}")
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
        self._emit_progress(progress, f"write finished: status={summary.status}")
        return summary

    async def _run_single_symbol_write(
        self,
        target_symbols: list[str],
        trade_dates: list[str],
        started_at: str,
        reporter: StatusReporter,
        progress: Callable[[str], None],
    ) -> JobRunSummary:
        if len(target_symbols) != 1:
            raise ValueError("single mode requires exactly one target symbol")

        ts_code = target_symbols[0]
        failures: dict[str, str] = {}
        successes: list[str] = []
        start_date = min(trade_dates) if trade_dates else None
        end_date = max(trade_dates) if trade_dates else None

        if start_date is not None and end_date is not None:
            self._emit_progress(progress, f"syncing single symbol {ts_code}: {start_date} -> {end_date}")
            try:
                payload = await with_retries(
                    lambda: self._fetch_symbol_payload(ts_code, start_date, end_date),
                    max_retries=self.settings.max_retries,
                    base_delay=self.settings.retry_base_delay,
                    backoff_factor=self.settings.retry_backoff_factor,
                    jitter=self.settings.retry_jitter,
                )
                bundle = normalize_symbol_bundle(
                    ts_code=ts_code,
                    daily_rows=payload[0],
                    daily_basic_rows=payload[1],
                    moneyflow_rows=payload[2],
                    indicator_rows=payload[3],
                )
                if self.market_repository is not None:
                    self._emit_progress(
                        progress,
                        f"persisting {len(bundle.market_rows)} market row(s) and {len(bundle.indicator_rows)} indicator row(s)",
                    )
                    await self.market_repository.upsert_daily_market(bundle.market_rows)
                    await self.market_repository.upsert_daily_indicators(bundle.indicator_rows)
                successes = [ts_code]
            except StockCacheError as exc:
                failures[ts_code] = str(exc)
                self._emit_progress(progress, f"single symbol {ts_code} failed: {exc}")
            except Exception as exc:
                failures[ts_code] = str(exc)
                self._emit_progress(progress, f"single symbol {ts_code} failed: {exc}")

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
        self._emit_progress(progress, f"write finished: status={summary.status}")
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

    async def _fetch_symbol_payload(
        self,
        ts_code: str,
        start_date: str,
        end_date: str,
    ) -> tuple[
        list[dict[str, object]],
        list[dict[str, object]],
        list[dict[str, object]],
        list[dict[str, object]],
    ]:
        return (
            self.primary_provider.fetch_daily(ts_code, start_date, end_date),
            self.primary_provider.fetch_daily_basic(ts_code, start_date, end_date),
            self.primary_provider.fetch_moneyflow(ts_code, start_date, end_date),
            self.primary_provider.fetch_indicators(ts_code, start_date, end_date),
        )

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

    def _trade_dates(self, write_range: WriteDateRange | None = None) -> list[str]:
        if write_range is not None and write_range.start_date is not None and write_range.end_date is not None:
            trade_dates = list(
                self.primary_provider.fetch_trade_dates_in_range(
                    write_range.start_date,
                    write_range.end_date,
                )
            )
        else:
            end_date = self.now_provider().strftime("%Y%m%d")
            lookback_trading_days = (
                write_range.lookback_trading_days
                if write_range is not None and write_range.lookback_trading_days is not None
                else self.settings.default_lookback_trading_days
            )
            trade_dates = list(
                self.primary_provider.fetch_recent_trade_dates(
                    end_date,
                    lookback_trading_days,
                )
            )
        if not trade_dates:
            raise ValueError("provider returned no trade dates for requested write window")
        return trade_dates

    @staticmethod
    def _emit_progress(progress: Callable[[str], None], message: str) -> None:
        progress(message)
