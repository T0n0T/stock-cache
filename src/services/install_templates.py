"""Template helpers for the install-skill workflow."""

from __future__ import annotations

from pathlib import Path


def _repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def _read_repo_skill(*parts: str) -> str:
    return (_repo_root() / "skills" / Path(*parts)).read_text(encoding="utf-8")


def _read_runtime_asset(*parts: str) -> str:
    return (_repo_root() / "runtime" / Path(*parts)).read_text(encoding="utf-8")


def render_installed_read_skill() -> str:
    return _read_repo_skill("stock-cache-read", "SKILL.md")


def render_installed_write_skill() -> str:
    return _read_repo_skill("stock-cache-write", "SKILL.md")


def render_installed_readme() -> str:
    return """# Stock Cache Standalone Home

This directory lives at `~/.agents/skills/stock-cache` and contains the shared runtime assets for both installed skills.

- `compose.yml`: copied from this repository so Docker Compose can start PostgreSQL.
- `.env`: captures `POSTGRES_DSN`, the provided `TUSHARE_TOKEN`, `STATUS_FILE_PATH`, and `INDEX_LIST_PATH`.
- `.runtime/`: stores PostgreSQL data (`.runtime/pgsql`), status files, and the installed `default-indexes.csv` list that controls which indexes are synced.

Both `stock-cache-read` and `stock-cache-write` cd into this directory before executing Docker Compose or the global `stock-cache` CLI so everything works without the repository checkout.
"""


def render_shared_env(token: str) -> str:
    return """POSTGRES_DSN=postgresql://postgres:postgres@127.0.0.1:5432/stock_cache
TUSHARE_TOKEN=%s
DEFAULT_LOOKBACK_TRADING_DAYS=90
STATUS_FILE_PATH=.runtime/last-write-status.txt
INDEX_LIST_PATH=.runtime/default-indexes.csv
""" % token


def render_default_indexes_csv() -> str:
    return _read_runtime_asset("default-indexes.csv")
