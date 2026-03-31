import asyncio
from dataclasses import asdict
import json

import typer

from stock_cache.use_cases.read_raw import ReadRawMarketDataUseCase
from stock_cache.use_cases.read_screen import ReadScreeningResultsUseCase

app = typer.Typer(help="Cache A-share market data into PostgreSQL.")

read_app = typer.Typer(help="Read cached data.")
app.add_typer(read_app, name="read")


@app.callback()
def main(ctx: typer.Context) -> None:
    if ctx.obj is None:
        ctx.obj = {}


@app.command()
def write(
    ctx: typer.Context,
    mode: str = typer.Option(..., "--mode"),
) -> None:
    """Write cached market data."""
    use_case = ctx.obj.get("write_use_case")
    if use_case is None:
        raise typer.BadParameter("write_use_case is not configured in typer context")
    payload = asyncio.run(use_case.run(mode=mode))
    typer.echo(json.dumps(asdict(payload), default=str))


@read_app.command("raw")
def read_raw(
    ctx: typer.Context,
    ts_code: str = typer.Option(..., "--ts-code"),
    start_date: str = typer.Option(..., "--start-date"),
    end_date: str = typer.Option(..., "--end-date"),
) -> None:
    """Read raw cached market data."""
    use_case = ctx.obj.get("read_raw_use_case")
    if not isinstance(use_case, ReadRawMarketDataUseCase):
        raise typer.BadParameter("read_raw_use_case is not configured in typer context")
    payload = asyncio.run(use_case.run(ts_code=ts_code, start_date=start_date, end_date=end_date))
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
    use_case = ctx.obj.get("read_screen_use_case")
    if not isinstance(use_case, ReadScreeningResultsUseCase):
        raise typer.BadParameter("read_screen_use_case is not configured in typer context")
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
    payload = asyncio.run(use_case.run(trade_date=trade_date, filters=filters))
    typer.echo(json.dumps(payload, default=str))
