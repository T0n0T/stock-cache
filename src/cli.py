import asyncio
from dataclasses import asdict
import json

import typer

from config import Settings
from db.init import initialize_schema
from db.pool import create_pool
from providers.tushare_adapter import TushareAdapter
from repositories.instruments import InstrumentRepository
from repositories.job_runs import JobRunRepository
from repositories.market_data import MarketDataRepository
from services.indicators import IndicatorService
from use_cases.read_raw import ReadRawMarketDataUseCase
from use_cases.read_screen import ReadScreeningResultsUseCase
from use_cases.write_market_data import WriteMarketDataUseCase

app = typer.Typer(help="Cache A-share market data into PostgreSQL.")

read_app = typer.Typer(help="Read cached data.")
app.add_typer(read_app, name="read")


@app.callback()
def main(ctx: typer.Context) -> None:
    if ctx.obj is None:
        ctx.obj = {}


async def _build_market_repository() -> tuple[MarketDataRepository, object]:
    settings = Settings()
    pool = await create_pool(settings.postgres_dsn)
    return MarketDataRepository(pool), pool


async def _run_write(mode: str, injected_use_case: object | None) -> object:
    use_case = injected_use_case
    if use_case is None:
        settings = Settings()
        pool = await create_pool(settings.postgres_dsn)
        repository = MarketDataRepository(pool)
        instrument_repository = InstrumentRepository(pool)
        job_run_repository = JobRunRepository(pool)
        primary_provider = TushareAdapter(
            settings.tushare_token,
            timeout_seconds=settings.request_timeout_seconds,
        )
        use_case = WriteMarketDataUseCase(
            settings=settings,
            primary_provider=primary_provider,
            market_repository=repository,
            instrument_repository=instrument_repository,
            job_run_repository=job_run_repository,
        )
        try:
            return await use_case.run(mode=mode)
        finally:
            await pool.close()
    return await use_case.run(mode=mode)


async def _run_init_db() -> dict[str, object]:
    settings = Settings()
    pool = await create_pool(settings.postgres_dsn)
    try:
        return await initialize_schema(pool)
    finally:
        await pool.close()


async def _run_read_raw(ts_code: str, start_date: str, end_date: str, injected_use_case: object | None) -> dict[str, object]:
    use_case = injected_use_case
    if not isinstance(use_case, ReadRawMarketDataUseCase):
        repository, pool = await _build_market_repository()
        use_case = ReadRawMarketDataUseCase(repository)
        try:
            return await use_case.run(ts_code=ts_code, start_date=start_date, end_date=end_date)
        finally:
            await pool.close()
    return await use_case.run(ts_code=ts_code, start_date=start_date, end_date=end_date)


async def _run_read_screen(trade_date: str, filters: dict[str, object], injected_use_case: object | None) -> dict[str, object]:
    use_case = injected_use_case
    if not isinstance(use_case, ReadScreeningResultsUseCase):
        settings = Settings()
        repository, pool = await _build_market_repository()
        indicator_service = IndicatorService(
            allow_online_backfill=settings.allow_indicator_backfill_on_read,
            enable_local_fallback=settings.enable_local_indicator_fallback,
        )
        use_case = ReadScreeningResultsUseCase(repository, indicator_service=indicator_service)
        try:
            return await use_case.run(trade_date=trade_date, filters=filters)
        finally:
            await pool.close()
    return await use_case.run(trade_date=trade_date, filters=filters)


@app.command("init-db")
def init_db() -> None:
    """Initialize PostgreSQL schema."""
    payload = asyncio.run(_run_init_db())
    typer.echo(json.dumps(payload, default=str))
    if payload["status"] != "ok":
        raise typer.Exit(code=1)


@app.command()
def write(
    ctx: typer.Context,
    mode: str = typer.Option(..., "--mode"),
) -> None:
    """Write cached market data."""
    payload = asyncio.run(_run_write(mode=mode, injected_use_case=ctx.obj.get("write_use_case")))
    typer.echo(json.dumps(asdict(payload), default=str))


@read_app.command("raw")
def read_raw(
    ctx: typer.Context,
    ts_code: str = typer.Option(..., "--ts-code"),
    start_date: str = typer.Option(..., "--start-date"),
    end_date: str = typer.Option(..., "--end-date"),
) -> None:
    """Read raw cached market data."""
    payload = asyncio.run(
        _run_read_raw(
            ts_code=ts_code,
            start_date=start_date,
            end_date=end_date,
            injected_use_case=ctx.obj.get("read_raw_use_case"),
        )
    )
    typer.echo(json.dumps(payload, default=str))


@read_app.command("screen")
def read_screen(
    ctx: typer.Context,
    trade_date: str = typer.Option(..., "--trade-date"),
    pct_chg_gte: float | None = typer.Option(None, "--pct-chg-gte"),
    turnover_rate_gte: float | None = typer.Option(None, "--turnover-rate-gte"),
    total_mv_gte: float | None = typer.Option(None, "--total-mv-gte"),
    total_mv_lte: float | None = typer.Option(None, "--total-mv-lte"),
    macd_gte: float | None = typer.Option(None, "--macd-gte"),
    kdj_j_gte: float | None = typer.Option(None, "--kdj-j-gte"),
) -> None:
    """Read screened cached market data."""
    filters = {
        key: value
        for key, value in {
            "pct_chg_gte": pct_chg_gte,
            "turnover_rate_gte": turnover_rate_gte,
            "total_mv_gte": total_mv_gte,
            "total_mv_lte": total_mv_lte,
            "macd_gte": macd_gte,
            "kdj_j_gte": kdj_j_gte,
        }.items()
        if value is not None
    }
    payload = asyncio.run(
        _run_read_screen(
            trade_date=trade_date,
            filters=filters,
            injected_use_case=ctx.obj.get("read_screen_use_case"),
        )
    )
    typer.echo(json.dumps(payload, default=str))
