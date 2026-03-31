---
name: stock-cache-write
description: Use when working in this repository and needing to populate or refresh the local PostgreSQL cache from Tushare, initialize the schema before syncing, inspect the latest write status file, or rerun the supported write flow with the project's CLI.
---

# Stock Cache Write

## Overview

Use the repository's Typer CLI from the repo root to push A-share data from Tushare into PostgreSQL.
Prefer `uv run stock-cache ...`; keep the CLI thin and do not invent alternate entry scripts.

## Preconditions

Run commands from the repository root.

Before writing data, make sure:

```bash
uv sync
docker compose -f config.yml up -d postgres
cp .env.example .env
```

Set at least these variables in `.env`:

- `POSTGRES_DSN`
- `TUSHARE_TOKEN`

Initialize the schema before the first write against a fresh database:

```bash
uv run stock-cache init-db
```

## Write Workflow

Run a normal cache refresh with:

```bash
uv run stock-cache write --mode full
```

The CLI also accepts:

```bash
uv run stock-cache write --mode failed-only
```

Current implementation note:

- The CLI accepts `full` and `failed-only`, but `WriteMarketDataUseCase.run()` does not currently branch on `mode`.
- Do not assume `failed-only` replays only the previous failure list unless the code changes.

## What To Read After A Write

The command prints a JSON summary to stdout. Expect fields like:

- `job_id`
- `status`
- `started_at`
- `finished_at`
- `total_symbols`
- `success_symbols`
- `failed_symbols`

Each run also overwrites the fixed status file at `STATUS_FILE_PATH`.
Default path:

```text
.runtime/last-write-status.txt
```

Read that file when you need a human-readable list of successes and failures.

## Common Command Sequence

Use this order when setting up or refreshing a local cache:

```bash
uv sync
docker compose -f config.yml up -d postgres
uv run stock-cache init-db
uv run stock-cache write --mode full
```

## Troubleshooting

If `init-db` or `write` fails immediately, check:

- PostgreSQL is reachable through `POSTGRES_DSN`
- `.env` exists in the repo root
- `TUSHARE_TOKEN` is set to a valid token

If you need to inspect the supported top-level commands, run:

```bash
uv run stock-cache --help
uv run stock-cache write --help
```
