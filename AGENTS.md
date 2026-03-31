# AGENTS

## Purpose

This repository is a small Python CLI for caching A-share market data into PostgreSQL and reading it back as JSON.

## Entry Points

- Primary CLI: `uv run stock-cache ...`
- Direct module entry: `uv run python -m cli ...`

Do not introduce alternate top-level entry scripts for the same workflow.

## Layout

- `src/cli.py`: Typer CLI entrypoint
- `src/config.py`: environment-backed settings
- `src/db/`: schema and PostgreSQL helpers
- `src/providers/`: upstream market-data adapters
- `src/repositories/`: persistence layer
- `src/services/`: normalization, retry, status-file helpers
- `src/use_cases/`: orchestration for write and read flows
- `tests/`: unit and integration coverage

The codebase uses a flat `src` layout. Keep imports like `from config import Settings` and `from repositories.market_data import MarketDataRepository`. Do not reintroduce a `src/stock_cache` package layout.

## Working Rules

- Keep the CLI thin. Business logic belongs in `use_cases`, `services`, and `repositories`.
- Preserve the existing layering. Avoid mixing database access into CLI handlers.
- Follow the current JSON contract for CLI output unless the task explicitly changes it.
- Prefer small, targeted edits over broad refactors.
- When changing schema or CLI behavior, update README in the same change.

## Verification

- For CLI or packaging changes, run at least `uv run stock-cache --help`.
- For Python logic changes, prefer targeted pytest commands first, then broader coverage if the touched area is stable.
- Some integration tests require a live PostgreSQL instance and can be unsuitable in restricted sandboxes. State clearly what you did and did not verify.

## Avoid

- Do not add placeholder scripts like `main.py` when the real entrypoint is already the Typer CLI.
- Do not duplicate README usage guidance inside code comments or tests.
- Do not silently change environment variable names or output field names.
