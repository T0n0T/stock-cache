import asyncio
import json

import typer

from stock_cache.use_cases.read_raw import ReadRawMarketDataUseCase

app = typer.Typer(help="Cache A-share market data into PostgreSQL.")

read_app = typer.Typer(help="Read cached data.")
app.add_typer(read_app, name="read")


@app.callback()
def main(ctx: typer.Context) -> None:
    if ctx.obj is None:
        ctx.obj = {}


@app.command()
def write() -> None:
    """Write cached market data."""


@read_app.command("raw")
def read_raw(ctx: typer.Context, ts_code: str, start_date: str, end_date: str) -> None:
    """Read raw cached market data."""
    use_case = ctx.obj.get("read_raw_use_case")
    if not isinstance(use_case, ReadRawMarketDataUseCase):
        raise typer.BadParameter("read_raw_use_case is not configured in typer context")
    payload = asyncio.run(use_case.run(ts_code=ts_code, start_date=start_date, end_date=end_date))
    typer.echo(json.dumps(payload, default=str))


@read_app.command("screen")
def read_screen() -> None:
    """Read screened cached market data."""
