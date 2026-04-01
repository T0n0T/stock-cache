# Config Show All Settings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every setting defined in `src/config.py` configurable via `.env`, overridable by shell exports, and visible through `uv run stock-cache config show` as effective `ENV_NAME=value` lines.

**Architecture:** Keep `Settings` as the single source of truth for runtime configuration. Add a small config-export helper that derives environment variable aliases and stringifies effective values from a resolved `Settings` instance, then reuse it from the CLI show command and tests.

**Tech Stack:** Python 3.13, Typer, pydantic-settings, pytest

---

### Task 1: Add failing tests for full config display

**Files:**
- Modify: `tests/test_config.py`
- Modify: `tests/test_cli.py`

- [ ] **Step 1: Write the failing tests**

```python
def test_settings_env_variable_names_include_all_declared_fields() -> None:
    assert settings_env_variable_names() == (
        "POSTGRES_DSN",
        "TUSHARE_TOKEN",
        "MAX_CONCURRENCY",
    )


def test_cli_config_show_prints_all_effective_values(...) -> None:
    result = runner.invoke(app, ["--env-file", str(env_file), "config", "show"])
    assert "MAX_CONCURRENCY=11\n" in result.stdout
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `uv run pytest tests/test_config.py tests/test_cli.py -q`
Expected: FAIL because the helper does not exist and `config show` only prints two variables.

- [ ] **Step 3: Write minimal implementation**

```python
def settings_env_variable_names() -> tuple[str, ...]:
    return tuple(...)
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `uv run pytest tests/test_config.py tests/test_cli.py -q`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add tests/test_config.py tests/test_cli.py src/config.py src/cli.py
git commit -m "feat: show all effective config values"
```

### Task 2: Update docs for full env coverage

**Files:**
- Modify: `README.md`
- Modify: `.env.example`

- [ ] **Step 1: Add docs changes**

```md
`config show` prints all effective runtime configuration values.
```

- [ ] **Step 2: Run CLI help verification**

Run: `uv run stock-cache --help`
Expected: PASS and help output includes the CLI commands.

- [ ] **Step 3: Commit**

```bash
git add README.md .env.example docs/superpowers/plans/2026-04-01-config-show-all-settings.md
git commit -m "docs: document full config output"
```
