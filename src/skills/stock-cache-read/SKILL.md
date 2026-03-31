---
name: stock-cache-read
description: Read cached A-share market data and screening results as JSON.
---

Use `uv run stock-cache read raw --ts-code 000001.SZ --start-date 2026-01-01 --end-date 2026-03-30` and consume JSON from stdout.
Use `uv run stock-cache read screen --trade-date 2026-03-30 --pct-chg-gte 5 --turnover-rate-gte 3` for screening output.
