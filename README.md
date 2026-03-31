# Stock Cache

## Setup

1. Copy `.env.example` to `.env`
2. Fill `POSTGRES_DSN` and `TUSHARE_TOKEN`
3. Create the PostgreSQL tables from `src/stock_cache/db/schema.sql`
4. Run `uv sync`

## Commands

- `uv run stock-cache write --mode full`
- `uv run stock-cache write --mode failed-only`
- `uv run stock-cache read raw --ts-code 000001.SZ --start-date 2026-01-01 --end-date 2026-03-30`
- `uv run stock-cache read screen --trade-date 2026-03-30 --pct-chg-gte 5 --turnover-rate-gte 3`
