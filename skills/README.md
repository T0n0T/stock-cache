# Stock Cache Standalone Home

This directory lives at `~/.agents/skills/stock-cache` and contains the shared runtime assets for both installed skills.

- `compose.yml`: copied from this repository so Docker Compose can start PostgreSQL.
- `.env`: captures `POSTGRES_DSN`, the provided `TUSHARE_TOKEN`, and `STATUS_FILE_PATH` pointed at `.runtime/last-write-status.txt`.
- `.runtime/`: stores PostgreSQL data (`.runtime/pgsql`) and status files.

Both `stock-cache-read` and `stock-cache-write` cd into this directory before executing Docker Compose or the global `stock-cache` CLI so everything works without the repository checkout.
