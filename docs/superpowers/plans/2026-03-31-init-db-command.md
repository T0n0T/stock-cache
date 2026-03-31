# Init DB Command Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an explicit `stock-cache init-db` command that executes the schema file, reports which core tables were created vs already present, and fails if required tables are still missing afterward.

**Architecture:** Keep the CLI thin by moving schema execution and table inspection into a small database initialization helper. The CLI command should only call that helper, print JSON, and set exit status based on the helper result.

**Tech Stack:** Python 3.13, asyncpg, Typer, pytest, pytest-asyncio

---

### Task 1: Lock CLI behavior with tests

**Files:**
- Modify: `tests/test_cli.py`
- Modify: `tests/integration/test_postgres_smoke.py`

- [ ] Add a unit test for a new `init-db` CLI command that prints JSON with `status`, `created_tables`, `already_present`, and `missing`.
- [ ] Add a live integration test that drops the four core tables, runs `init-db`, and verifies the command recreates them.
- [ ] Add a second live integration test that reruns `init-db` and verifies it reports the tables as already present.
- [ ] Run:
```bash
uv run pytest tests/test_cli.py tests/integration/test_postgres_smoke.py -v
```

### Task 2: Implement schema initialization helper and CLI entrypoint

**Files:**
- Create: `src/db/init.py`
- Modify: `src/cli.py`

- [ ] Add a helper that loads `src/db/schema.sql`, executes it, inspects `pg_tables`, and returns a structured result for the four required tables.
- [ ] Add `init-db` to the Typer app and return the helper payload as JSON.
- [ ] Exit non-zero if any required table is still missing after schema execution.
- [ ] Run:
```bash
uv run pytest tests/test_cli.py tests/integration/test_postgres_smoke.py -v
```

### Task 3: Document and verify

**Files:**
- Modify: `README.md`

- [ ] Update setup and command examples to use `stock-cache init-db`.
- [ ] Run:
```bash
uv run pytest
```
