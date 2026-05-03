import asyncio
from dataclasses import asdict, is_dataclass
from datetime import datetime
from enum import StrEnum
import json
from pathlib import Path
from typing import Callable

import click
import typer

from config import Settings, resolve_runtime_env
from db.init import initialize_schema
from db.pool import DatabasePrecheckError, create_pool
from providers.tushare_adapter import TushareAdapter
from repositories.instruments import InstrumentLookupError, InstrumentRepository
from repositories.job_runs import JobRunRepository
from repositories.market_data import MarketDataRepository
from services.indicators import IndicatorService
from services.installer import SkillInstaller
from use_cases.delete_by_date import DeleteByDateUseCase
from use_cases.install_skill import InstallSkillUseCase
from use_cases.read_raw import ReadRawMarketDataUseCase
from use_cases.read_screen import ReadScreeningResultsUseCase
from use_cases.stats_date_range import StatsDateRangeUseCase
from use_cases.write_market_data import WriteDateRange, WriteMarketDataUseCase

app = typer.Typer(help="Cache A-share market data into PostgreSQL.")

read_app = typer.Typer(help="Read cached data.")
app.add_typer(read_app, name="read")
stats_app = typer.Typer(help="Inspect cached data.")
app.add_typer(stats_app, name="stats")
delete_app = typer.Typer(help="Delete cached data.")
app.add_typer(delete_app, name="delete")
config_app = typer.Typer(help="Inspect runtime configuration.")
app.add_typer(config_app, name="config")


class WriteMode(StrEnum):
    FULL = "full"
    SINGLE = "single"


def _summarize_write_payload(payload: object) -> object:
    if is_dataclass(payload):
        payload = asdict(payload)
    if not isinstance(payload, dict):
        return payload
    success_symbols = payload.get("success_symbols")
    failed_symbols = payload.get("failed_symbols")
    if not isinstance(success_symbols, list) or not isinstance(failed_symbols, dict):
        return payload
    return {
        "job_id": payload.get("job_id"),
        "status": payload.get("status"),
        "started_at": payload.get("started_at"),
        "finished_at": payload.get("finished_at"),
        "total_symbols": payload.get("total_symbols"),
        "success_count": len(success_symbols),
        "failed_count": len(failed_symbols),
    }


@app.callback()
def main(
    ctx: typer.Context,
    env_file: Path | None = typer.Option(
        None,
        "--env-file",
        exists=False,
        dir_okay=False,
        resolve_path=True,
        help="Load settings from the specified .env file. Shell environment variables still take precedence.",
    ),
) -> None:
    if ctx.obj is None:
        ctx.obj = {}
    if env_file is not None and not env_file.exists():
        raise typer.BadParameter(f"Env file not found: {env_file}", param_hint="--env-file")
    ctx.obj["env_file"] = env_file


def _get_settings(ctx: typer.Context | None = None) -> Settings:
    if ctx is None:
        ctx = click.get_current_context(silent=True)
    env_file = None
    if ctx is not None and ctx.obj is not None:
        env_file = ctx.obj.get("env_file")
    return Settings.from_env_file(env_file)


async def _build_market_repository(settings: Settings) -> tuple[MarketDataRepository, object]:
    pool = await create_pool(settings.postgres_dsn)
    return MarketDataRepository(pool, write_batch_size=settings.write_batch_size), pool


async def _run_write(
    mode: WriteMode | str,
    ts_code: str | None,
    name: str | None,
    write_range: WriteDateRange | None,
    injected_use_case: object | None,
    progress: Callable[[str], None] | None,
) -> object:
    mode = WriteMode(mode)
    use_case = injected_use_case
    pool = None
    settings = None
    try:
        symbols = None
        if use_case is None or (mode is WriteMode.SINGLE and name is not None):
            settings = _get_settings()
            pool = await create_pool(settings.postgres_dsn)

        if mode is WriteMode.SINGLE:
            if name is not None:
                instrument_repository = InstrumentRepository(pool)
                instrument = await instrument_repository.find_by_name(name)
                symbols = [instrument.ts_code]
            else:
                symbols = [ts_code]

        if use_case is None:
            if settings is None:
                settings = _get_settings()
            repository = MarketDataRepository(pool, write_batch_size=settings.write_batch_size)
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
        return await use_case.run(mode=mode.value, symbols=symbols, write_range=write_range, progress=progress)
    finally:
        if pool is not None:
            await pool.close()


async def _run_init_db() -> dict[str, object]:
    settings = _get_settings()
    pool = await create_pool(settings.postgres_dsn)
    try:
        return await initialize_schema(pool)
    finally:
        await pool.close()


async def _run_read_raw(
    ts_code: str | None,
    name: str | None,
    start_date: str,
    end_date: str,
    injected_use_case: object | None,
) -> dict[str, object]:
    use_case = injected_use_case
    resolved_ts_code = ts_code
    pool = None
    try:
        if name is not None or not isinstance(use_case, ReadRawMarketDataUseCase):
            settings = _get_settings()
            pool = await create_pool(settings.postgres_dsn)

        if name is not None:
            instrument_repository = InstrumentRepository(pool)
            instrument = await instrument_repository.find_by_name(name)
            resolved_ts_code = instrument.ts_code

        if not isinstance(use_case, ReadRawMarketDataUseCase):
            repository = MarketDataRepository(pool, write_batch_size=settings.write_batch_size)
            use_case = ReadRawMarketDataUseCase(repository)

        return await use_case.run(ts_code=resolved_ts_code, start_date=start_date, end_date=end_date)
    finally:
        if pool is not None:
            await pool.close()


async def _run_read_screen(
    trade_date: str,
    filters: dict[str, object],
    injected_use_case: object | None,
) -> dict[str, object]:
    use_case = injected_use_case
    if not isinstance(use_case, ReadScreeningResultsUseCase):
        settings = _get_settings()
        repository, pool = await _build_market_repository(settings)
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


async def _run_stats_date_range(injected_use_case: object | None) -> dict[str, object]:
    use_case = injected_use_case
    if use_case is None:
        settings = _get_settings()
        pool = await create_pool(settings.postgres_dsn)
        repository = MarketDataRepository(pool, write_batch_size=settings.write_batch_size)
        provider = TushareAdapter(
            settings.tushare_token,
            timeout_seconds=settings.request_timeout_seconds,
        )
        use_case = StatsDateRangeUseCase(repository, provider)
        try:
            return await use_case.run()
        finally:
            await pool.close()
    return await use_case.run()


async def _run_delete_by_date(
    start_date: str,
    end_date: str,
    injected_use_case: object | None,
) -> dict[str, object]:
    use_case = injected_use_case
    if use_case is None:
        settings = _get_settings()
        repository, pool = await _build_market_repository(settings)
        use_case = DeleteByDateUseCase(repository)
        try:
            return await use_case.run(start_date=start_date, end_date=end_date)
        finally:
            await pool.close()
    return await use_case.run(start_date=start_date, end_date=end_date)


async def _run_install_skill(
    token: str | None,
    force: bool,
    injected_use_case: object | None,
) -> dict[str, object]:
    use_case = injected_use_case
    if use_case is None:
        use_case = InstallSkillUseCase(
            installer=SkillInstaller(),
            repo_root=Path(__file__).resolve().parent.parent,
        )
    return await use_case.run(token=token, force=force)


@app.command("init-db")
def init_db() -> None:
    """Initialize PostgreSQL schema."""
    try:
        payload = asyncio.run(_run_init_db())
        typer.echo(json.dumps(payload, default=str))
        if payload["status"] != "ok":
            raise typer.Exit(code=1)
    except DatabasePrecheckError as exc:
        _exit_with_json_error("postgres_unreachable", str(exc))


@app.command()
def write(
    ctx: typer.Context,
    mode: WriteMode = typer.Option(
        ...,
        "--mode",
        help=(
            "Write mode. 'full' syncs all active instruments in the selected window. "
            "'single' syncs exactly one instrument selected by --ts-code or --name."
        ),
    ),
    ts_code: str | None = typer.Option(
        None,
        "--ts-code",
        help="Target ts_code for --mode single, for example 000001.SZ.",
    ),
    name: str | None = typer.Option(
        None,
        "--name",
        help="Target instrument name for --mode single. Resolved from the cached instruments table.",
    ),
    lookback_trading_days: int | None = typer.Option(None, "--lookback-trading-days", min=1),
    start_date: str | None = typer.Option(None, "--start-date"),
    end_date: str | None = typer.Option(None, "--end-date"),
) -> None:
    """Write cached market data."""
    _validate_write_selector(mode=mode, ts_code=ts_code, name=name)
    write_range = _validate_write_range(
        lookback_trading_days=lookback_trading_days,
        start_date=start_date,
        end_date=end_date,
    )
    try:
        payload = asyncio.run(
            _run_write(
                mode=mode,
                ts_code=ts_code,
                name=name,
                write_range=write_range,
                injected_use_case=ctx.obj.get("write_use_case"),
                progress=_emit_write_progress,
            )
        )
        payload = _summarize_write_payload(payload)
        typer.echo(json.dumps(payload, default=str))
    except DatabasePrecheckError as exc:
        _exit_with_json_error("postgres_unreachable", str(exc))
    except InstrumentLookupError as exc:
        raise typer.BadParameter(str(exc)) from exc


@app.command("install-skill")
def install_skill(
    ctx: typer.Context,
    token: str | None = typer.Option(None, "--token"),
    force: bool = typer.Option(False, "--force"),
) -> None:
    """Install the standalone global CLI and skill files."""
    try:
        effective_token = token if token is not None else typer.prompt("TUSHARE token", hide_input=True)
        payload = asyncio.run(
            _run_install_skill(
                token=effective_token,
                force=force,
                injected_use_case=ctx.obj.get("install_skill_use_case"),
            )
        )
        _emit_install_skill_result(payload)
    except ValueError as exc:
        typer.echo(f"Error: {exc}")
        raise typer.Exit(code=1)


@read_app.command("raw")
def read_raw(
    ctx: typer.Context,
    ts_code: str | None = typer.Option(None, "--ts-code"),
    name: str | None = typer.Option(None, "--name"),
    start_date: str = typer.Option(..., "--start-date"),
    end_date: str = typer.Option(..., "--end-date"),
) -> None:
    """Read raw cached market data."""
    _validate_raw_lookup_selector(ts_code=ts_code, name=name)
    try:
        payload = asyncio.run(
            _run_read_raw(
                ts_code=ts_code,
                name=name,
                start_date=start_date,
                end_date=end_date,
                injected_use_case=ctx.obj.get("read_raw_use_case"),
            )
        )
        typer.echo(json.dumps(payload, default=str))
    except DatabasePrecheckError as exc:
        _exit_with_json_error("postgres_unreachable", str(exc))
    except InstrumentLookupError as exc:
        raise typer.BadParameter(str(exc)) from exc


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
    try:
        payload = asyncio.run(
            _run_read_screen(
                trade_date=trade_date,
                filters=filters,
                injected_use_case=ctx.obj.get("read_screen_use_case"),
            )
        )
        typer.echo(json.dumps(payload, default=str))
    except DatabasePrecheckError as exc:
        _exit_with_json_error("postgres_unreachable", str(exc))


@stats_app.command("date-range")
def stats_date_range(ctx: typer.Context) -> None:
    """Show queryable cached trade-date segments."""
    try:
        payload = asyncio.run(_run_stats_date_range(injected_use_case=ctx.obj.get("stats_date_range_use_case")))
        typer.echo(json.dumps(payload, default=str))
    except DatabasePrecheckError as exc:
        _exit_with_json_error("postgres_unreachable", str(exc))


@delete_app.command("by-date")
def delete_by_date(
    ctx: typer.Context,
    trade_date: str | None = typer.Option(None, "--trade-date"),
    start_date: str | None = typer.Option(None, "--start-date"),
    end_date: str | None = typer.Option(None, "--end-date"),
) -> None:
    """Delete cached rows by trade date or date range."""
    normalized_start, normalized_end = _validate_delete_range(
        trade_date=trade_date,
        start_date=start_date,
        end_date=end_date,
    )
    try:
        payload = asyncio.run(
            _run_delete_by_date(
                start_date=normalized_start,
                end_date=normalized_end,
                injected_use_case=ctx.obj.get("delete_by_date_use_case"),
            )
        )
        typer.echo(json.dumps(payload, default=str))
    except DatabasePrecheckError as exc:
        _exit_with_json_error("postgres_unreachable", str(exc))


@config_app.command("show")
def config_show(ctx: typer.Context) -> None:
    """Show effective runtime configuration values."""
    env_file = ctx.obj.get("env_file") if ctx.obj is not None else None
    values = resolve_runtime_env(env_file)
    for name, value in values.items():
        typer.echo(f"{name}={value}")


def _validate_raw_lookup_selector(ts_code: str | None, name: str | None) -> None:
    if (ts_code is None) == (name is None):
        raise typer.BadParameter("Exactly one of --ts-code or --name must be provided.")


def _validate_write_selector(mode: WriteMode, ts_code: str | None, name: str | None) -> None:
    if mode is WriteMode.SINGLE:
        if (ts_code is None) == (name is None):
            raise typer.BadParameter("Exactly one of --ts-code or --name must be provided for --mode single.")
        return
    if ts_code is not None or name is not None:
        raise typer.BadParameter("--ts-code/--name can only be used with --mode single.")


def _validate_write_range(
    lookback_trading_days: int | None,
    start_date: str | None,
    end_date: str | None,
) -> WriteDateRange | None:
    if lookback_trading_days is not None and (start_date is not None or end_date is not None):
        raise typer.BadParameter("--lookback-trading-days cannot be combined with --start-date/--end-date.")
    if (start_date is None) != (end_date is None):
        raise typer.BadParameter("--start-date and --end-date must be provided together.")
    if lookback_trading_days is not None:
        return WriteDateRange(lookback_trading_days=lookback_trading_days)
    if start_date is None or end_date is None:
        return None

    normalized_start = _normalize_cli_date(start_date)
    normalized_end = _normalize_cli_date(end_date)
    if normalized_start > normalized_end:
        raise typer.BadParameter("--start-date must be earlier than or equal to --end-date.")
    return WriteDateRange(start_date=normalized_start, end_date=normalized_end)


def _validate_delete_range(
    trade_date: str | None,
    start_date: str | None,
    end_date: str | None,
) -> tuple[str, str]:
    if trade_date is not None and (start_date is not None or end_date is not None):
        raise typer.BadParameter("--trade-date cannot be combined with --start-date/--end-date.")
    if trade_date is not None:
        normalized_trade_date = _normalize_cli_date(trade_date)
        return normalized_trade_date, normalized_trade_date
    if (start_date is None) != (end_date is None):
        raise typer.BadParameter("--start-date and --end-date must be provided together.")
    if start_date is None or end_date is None:
        raise typer.BadParameter("Provide --trade-date or --start-date/--end-date.")

    normalized_start = _normalize_cli_date(start_date)
    normalized_end = _normalize_cli_date(end_date)
    if normalized_start > normalized_end:
        raise typer.BadParameter("--start-date must be earlier than or equal to --end-date.")
    return normalized_start, normalized_end


def _normalize_cli_date(value: str) -> str:
    try:
        return datetime.strptime(value, "%Y-%m-%d").strftime("%Y%m%d")
    except ValueError:
        try:
            return datetime.strptime(value, "%Y%m%d").strftime("%Y%m%d")
        except ValueError as exc:
            raise typer.BadParameter(f"Invalid date '{value}'. Use YYYY-MM-DD.") from exc


def _emit_write_progress(message: str) -> None:
    typer.echo(message, err=True)


def _emit_install_skill_result(payload: dict[str, object]) -> None:
    data = payload.get("data", {})
    typer.echo("Installed stock-cache global CLI and standalone skills.")
    if isinstance(data, dict):
        shared_home = data.get("shared_home")
        if shared_home:
            typer.echo(f"Shared home: {shared_home}")

        skills = data.get("skills")
        if isinstance(skills, list) and skills:
            typer.echo("Installed skills:")
            for skill in skills:
                typer.echo(f"- {skill}")

        compose_file = data.get("compose_file")
        if compose_file:
            typer.echo(f"Compose file: {compose_file}")

        default_indexes_file = data.get("default_indexes_file")
        if default_indexes_file:
            typer.echo(f"Default index list: {default_indexes_file}")

    next_steps = payload.get("next_steps")
    if isinstance(next_steps, list) and next_steps:
        typer.echo("Next steps:")
        for step in next_steps:
            typer.echo(f"- {step}")


def _exit_with_json_error(error: str, message: str) -> None:
    typer.echo(json.dumps({"status": "error", "error": error, "message": message}))
    raise typer.Exit(code=1)
