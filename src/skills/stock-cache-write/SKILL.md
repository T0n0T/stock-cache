---
name: stock-cache-write
description: Refresh A-share market data into PostgreSQL with status-file reporting.
---

Use `uv run stock-cache write --mode full` and read the fixed status file after completion.
Use `uv run stock-cache write --mode failed-only` to retry the failed symbol list from the last status file.
