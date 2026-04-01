# Global Skill Install Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a supported installation flow that globally installs the current `stock-cache` CLI with `uv`, copies standalone `stock-cache-read` and `stock-cache-write` skills into `~/.agents/skills`, and writes a shared standalone runtime home that does not depend on the repository checkout.

**Architecture:** Keep `src/cli.py` thin by adding a new `install-skill` command that delegates to a dedicated use case. Put filesystem, template, and `uv tool install` subprocess work into installer services so the command remains testable and repository layering stays consistent.

**Tech Stack:** Python 3.13, Typer, pathlib, subprocess, shutil, pytest, uv

---

### File Structure

**Create:**
- `src/use_cases/install_skill.py`
- `src/services/installer.py`
- `src/services/install_templates.py`
- `tests/use_cases/test_install_skill.py`
- `tests/services/test_installer.py`
- `scripts/install-global.sh`
- `docs/superpowers/plans/2026-04-01-global-skill-install.md`

**Modify:**
- `src/cli.py`
- `tests/test_cli.py`
- `README.md`
- `skills/stock-cache-read/SKILL.md`
- `skills/stock-cache-write/SKILL.md`

**Do not modify:**
- `src/db/schema.sql`
- existing read and write use-case contracts
- `pyproject.toml` entrypoints

### Task 1: Lock the CLI install contract with tests

**Files:**
- Modify: `tests/test_cli.py`

- [ ] **Step 1: Write the failing tests**

```python
class FakeInstallSkillUseCase:
    def __init__(self) -> None:
        self.calls: list[dict[str, object]] = []

    async def run(self, token: str | None, force: bool) -> dict[str, object]:
        self.calls.append({"token": token, "force": force})
        return {
            "status": "ok",
            "data": {
                "cli_installed": True,
                "cli_command": "stock-cache",
                "shared_home": "/home/example/.agents/skills/stock-cache",
                "skills": [
                    "/home/example/.agents/skills/stock-cache-read",
                    "/home/example/.agents/skills/stock-cache-write",
                ],
                "compose_file": "/home/example/.agents/skills/stock-cache/compose.yml",
                "token_written": True,
            },
            "next_steps": [
                "cd ~/.agents/skills/stock-cache",
                "docker compose up -d postgres",
                "stock-cache init-db",
            ],
        }


def test_cli_help_lists_install_skill_command() -> None:
    result = runner.invoke(app, ["--help"])

    assert result.exit_code == 0
    assert "install-skill" in result.stdout


def test_cli_install_skill_help_lists_token_and_force() -> None:
    result = runner.invoke(app, ["install-skill", "--help"])

    assert result.exit_code == 0
    assert "--token" in result.stdout
    assert "--force" in result.stdout


def test_cli_install_skill_passes_flags_to_use_case() -> None:
    use_case = FakeInstallSkillUseCase()

    result = runner.invoke(
        app,
        ["install-skill", "--token", "abc123", "--force"],
        obj={"install_skill_use_case": use_case},
    )

    assert result.exit_code == 0
    assert use_case.calls == [{"token": "abc123", "force": True}]
    payload = json.loads(result.stdout)
    assert payload["status"] == "ok"
    assert payload["data"]["cli_command"] == "stock-cache"
```

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest tests/test_cli.py -k "install_skill"`
Expected: FAIL because the CLI does not yet expose `install-skill`.

- [ ] **Step 3: Write minimal implementation**

```python
@app.command("install-skill")
def install_skill(
    ctx: typer.Context,
    token: str | None = typer.Option(None, "--token"),
    force: bool = typer.Option(False, "--force"),
) -> None:
    payload = asyncio.run(
        _run_install_skill(
            token=token,
            force=force,
            injected_use_case=ctx.obj.get("install_skill_use_case"),
        )
    )
    typer.echo(json.dumps(payload, default=str))
```

- [ ] **Step 4: Run test to verify it passes**

Run: `uv run pytest tests/test_cli.py -k "install_skill"`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add tests/test_cli.py src/cli.py
git commit -m "test: lock install-skill cli contract"
```

### Task 2: Add installer service tests for filesystem layout, overwrite policy, and uv invocation

**Files:**
- Create: `tests/services/test_installer.py`

- [ ] **Step 1: Write the failing tests**

```python
from pathlib import Path

from services.installer import InstallerPaths, SkillInstaller


class RecordingRunner:
    def __init__(self) -> None:
        self.calls: list[list[str]] = []

    def __call__(self, args: list[str]) -> None:
        self.calls.append(args)


def test_installer_creates_shared_home_and_skill_copies(tmp_path: Path) -> None:
    repo_root = tmp_path / "repo"
    repo_root.mkdir()
    (repo_root / "compose.yml").write_text("services:\n  postgres:\n    image: postgres:alpine\n", encoding="utf-8")
    (repo_root / "skills" / "stock-cache-read").mkdir(parents=True)
    (repo_root / "skills" / "stock-cache-write").mkdir(parents=True)
    (repo_root / "skills" / "stock-cache-read" / "SKILL.md").write_text("repo read", encoding="utf-8")
    (repo_root / "skills" / "stock-cache-write" / "SKILL.md").write_text("repo write", encoding="utf-8")

    home_dir = tmp_path / "home"
    runner = RecordingRunner()
    installer = SkillInstaller(home_dir=home_dir, run_command=runner)

    result = installer.install(
        repo_root=repo_root,
        token="token-1",
        force=False,
        read_skill_body="# read skill\n",
        write_skill_body="# write skill\n",
        shared_readme_body="# local readme\n",
        env_body="POSTGRES_DSN=postgresql://postgres:postgres@127.0.0.1:5432/stock_cache\nTUSHARE_TOKEN=token-1\nSTATUS_FILE_PATH=.runtime/last-write-status.txt\n",
    )

    assert (home_dir / ".agents" / "skills" / "stock-cache" / "compose.yml").exists()
    assert (home_dir / ".agents" / "skills" / "stock-cache" / "config" / "token.env").read_text(encoding="utf-8") == "TUSHARE_TOKEN=token-1\n"
    assert (home_dir / ".agents" / "skills" / "stock-cache-read" / "SKILL.md").is_symlink() is False
    assert runner.calls == [["uv", "tool", "install", "--from", str(repo_root), "stock-cache"]]
    assert result["data"]["token_written"] is True


def test_installer_requires_force_before_overwriting_existing_templates(tmp_path: Path) -> None:
    repo_root = tmp_path / "repo"
    repo_root.mkdir()
    (repo_root / "compose.yml").write_text("services:\n  postgres:\n    image: postgres:alpine\n", encoding="utf-8")

    home_dir = tmp_path / "home"
    runner = RecordingRunner()
    installer = SkillInstaller(home_dir=home_dir, run_command=runner)

    installer.install(
        repo_root=repo_root,
        token="token-1",
        force=False,
        read_skill_body="# read skill v1\n",
        write_skill_body="# write skill v1\n",
        shared_readme_body="# local readme v1\n",
        env_body="POSTGRES_DSN=postgresql://postgres:postgres@127.0.0.1:5432/stock_cache\nTUSHARE_TOKEN=token-1\nSTATUS_FILE_PATH=.runtime/last-write-status.txt\n",
    )

    with pytest.raises(FileExistsError):
        installer.install(
            repo_root=repo_root,
            token="token-1",
            force=False,
            read_skill_body="# read skill v2\n",
            write_skill_body="# write skill v2\n",
            shared_readme_body="# local readme v2\n",
            env_body="POSTGRES_DSN=postgresql://postgres:postgres@127.0.0.1:5432/stock_cache\nTUSHARE_TOKEN=token-1\nSTATUS_FILE_PATH=.runtime/last-write-status.txt\n",
        )


def test_installer_reinstalls_when_cli_already_present(tmp_path: Path) -> None:
    repo_root = tmp_path / "repo"
    repo_root.mkdir()
    (repo_root / "compose.yml").write_text("services:\n  postgres:\n    image: postgres:alpine\n", encoding="utf-8")

    home_dir = tmp_path / "home"
    runner = RecordingRunner()
    cli_marker = home_dir / ".agents" / "skills" / "stock-cache" / ".installed-cli"
    cli_marker.parent.mkdir(parents=True, exist_ok=True)
    cli_marker.write_text("installed\n", encoding="utf-8")

    installer = SkillInstaller(home_dir=home_dir, run_command=runner)
    installer.install(
        repo_root=repo_root,
        token="token-1",
        force=True,
        read_skill_body="# read skill\n",
        write_skill_body="# write skill\n",
        shared_readme_body="# local readme\n",
        env_body="POSTGRES_DSN=postgresql://postgres:postgres@127.0.0.1:5432/stock_cache\nTUSHARE_TOKEN=token-1\nSTATUS_FILE_PATH=.runtime/last-write-status.txt\n",
    )

    assert runner.calls == [["uv", "tool", "install", "--reinstall", "--from", str(repo_root), "stock-cache"]]


def test_installer_preserves_runtime_pgsql_directory_when_forcing(tmp_path: Path) -> None:
    repo_root = tmp_path / "repo"
    repo_root.mkdir()
    (repo_root / "compose.yml").write_text("services:\n  postgres:\n    image: postgres:alpine\n", encoding="utf-8")

    home_dir = tmp_path / "home"
    runner = RecordingRunner()
    installer = SkillInstaller(home_dir=home_dir, run_command=runner)

    installer.install(
        repo_root=repo_root,
        token="token-1",
        force=False,
        read_skill_body="# read skill v1\n",
        write_skill_body="# write skill v1\n",
        shared_readme_body="# local readme v1\n",
        env_body="POSTGRES_DSN=postgresql://postgres:postgres@127.0.0.1:5432/stock_cache\nTUSHARE_TOKEN=token-1\nSTATUS_FILE_PATH=.runtime/last-write-status.txt\n",
    )

    pgsql_file = home_dir / ".agents" / "skills" / "stock-cache" / ".runtime" / "pgsql" / "PG_VERSION"
    pgsql_file.write_text("17\n", encoding="utf-8")

    installer.install(
        repo_root=repo_root,
        token="token-2",
        force=True,
        read_skill_body="# read skill v2\n",
        write_skill_body="# write skill v2\n",
        shared_readme_body="# local readme v2\n",
        env_body="POSTGRES_DSN=postgresql://postgres:postgres@127.0.0.1:5432/stock_cache\nTUSHARE_TOKEN=token-2\nSTATUS_FILE_PATH=.runtime/last-write-status.txt\n",
    )

    assert pgsql_file.read_text(encoding="utf-8") == "17\n"
    assert (home_dir / ".agents" / "skills" / "stock-cache" / "config" / "token.env").read_text(encoding="utf-8") == "TUSHARE_TOKEN=token-2\n"
```

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest tests/services/test_installer.py -v`
Expected: FAIL because `services.installer` does not exist.

- [ ] **Step 3: Write minimal implementation**

```python
from dataclasses import dataclass
from pathlib import Path
import shutil


@dataclass(frozen=True)
class InstallerPaths:
    skills_root: Path
    shared_home: Path
    read_skill_dir: Path
    write_skill_dir: Path
    token_file: Path
    env_file: Path
    compose_file: Path
    runtime_dir: Path
    pgsql_dir: Path
    shared_readme: Path


class SkillInstaller:
    def __init__(self, home_dir: Path | None = None, run_command: object | None = None) -> None:
        self._home_dir = home_dir or Path.home()
        self._run_command = run_command or _run_command

    def install(
        self,
        repo_root: Path,
        token: str,
        force: bool,
        read_skill_body: str,
        write_skill_body: str,
        shared_readme_body: str,
        env_body: str,
    ) -> dict[str, object]:
        paths = self._paths()
        paths.pgsql_dir.mkdir(parents=True, exist_ok=True)
        cli_args = ["uv", "tool", "install", "--from", str(repo_root), "stock-cache"]
        self._run_command(cli_args)
```

- [ ] **Step 4: Expand implementation until the tests pass**

Required behavior:

- create `~/.agents/skills/stock-cache`
- create `config`, `.runtime`, and `.runtime/pgsql`
- copy `compose.yml`
- copy generated `SKILL.md` files as regular files
- write `config/token.env`
- write `.env`
- use `uv tool install --from /absolute/path/to/repo stock-cache` on first install
- use `uv tool install --reinstall --from /absolute/path/to/repo stock-cache` when the existing CLI marker indicates a prior install
- refuse template overwrite without `--force`
- preserve `.runtime/pgsql` contents when `--force` is used

- [ ] **Step 5: Run test to verify it passes**

Run: `uv run pytest tests/services/test_installer.py -v`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add tests/services/test_installer.py src/services/installer.py
git commit -m "feat: add standalone skill installer service"
```

### Task 3: Add install template helpers and a use case that orchestrates the installer

**Files:**
- Create: `src/services/install_templates.py`
- Create: `src/use_cases/install_skill.py`
- Create: `tests/use_cases/test_install_skill.py`

- [ ] **Step 1: Write the failing tests**

```python
from pathlib import Path

from use_cases.install_skill import InstallSkillUseCase


class FakeInstaller:
    def __init__(self) -> None:
        self.calls: list[dict[str, object]] = []

    def install(self, **kwargs: object) -> dict[str, object]:
        self.calls.append(kwargs)
        return {"status": "ok", "data": {"cli_installed": True}, "next_steps": []}


def test_install_skill_use_case_builds_template_payloads(tmp_path: Path) -> None:
    installer = FakeInstaller()
    use_case = InstallSkillUseCase(installer=installer, repo_root=tmp_path)

    payload = asyncio.run(use_case.run(token="abc123", force=True))

    assert payload["status"] == "ok"
    call = installer.calls[0]
    assert call["repo_root"] == tmp_path
    assert call["token"] == "abc123"
    assert call["force"] is True
    assert "stock-cache init-db" in call["read_skill_body"]
    assert "docker compose up -d postgres" in call["write_skill_body"]
    assert "TUSHARE_TOKEN=abc123" in call["env_body"]
```

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest tests/use_cases/test_install_skill.py -v`
Expected: FAIL because `InstallSkillUseCase` and template helpers do not exist.

- [ ] **Step 3: Write minimal implementation**

```python
from pathlib import Path

from services.install_templates import (
    render_installed_readme,
    render_installed_read_skill,
    render_installed_write_skill,
    render_shared_env,
)
from services.installer import SkillInstaller


class InstallSkillUseCase:
    def __init__(self, installer: SkillInstaller, repo_root: Path) -> None:
        self._installer = installer
        self._repo_root = repo_root

    async def run(self, token: str | None, force: bool) -> dict[str, object]:
        effective_token = _require_token(token)
        return self._installer.install(
            repo_root=self._repo_root,
            token=effective_token,
            force=force,
            read_skill_body=render_installed_read_skill(),
            write_skill_body=render_installed_write_skill(),
            shared_readme_body=render_installed_readme(),
            env_body=render_shared_env(effective_token),
        )
```

- [ ] **Step 4: Add template helpers and make the tests pass**

Required template content:

- installed read skill tells the agent to run from `~/.agents/skills/stock-cache`
- installed write skill tells the agent to run from `~/.agents/skills/stock-cache`
- installed README explains the standalone home layout
- generated `.env` contains:

```dotenv
POSTGRES_DSN=postgresql://postgres:postgres@127.0.0.1:5432/stock_cache
TUSHARE_TOKEN=abc123
STATUS_FILE_PATH=.runtime/last-write-status.txt
```

- [ ] **Step 5: Run test to verify it passes**

Run: `uv run pytest tests/use_cases/test_install_skill.py -v`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add tests/use_cases/test_install_skill.py src/use_cases/install_skill.py src/services/install_templates.py
git commit -m "feat: add install-skill use case and templates"
```

### Task 4: Wire the CLI command, bootstrap script, and interactive token fallback

**Files:**
- Modify: `src/cli.py`
- Create: `scripts/install-global.sh`
- Modify: `tests/test_cli.py`

- [ ] **Step 1: Write the failing tests for interactive token fallback**

```python
def test_cli_install_skill_prompts_for_token_when_missing(monkeypatch) -> None:
    prompts: list[str] = []

    def fake_prompt(text: str, hide_input: bool = False) -> str:
        prompts.append(text)
        assert hide_input is True
        return "prompt-token"

    class FakeInstallSkillUseCase:
        async def run(self, token: str | None, force: bool) -> dict[str, object]:
            assert token == "prompt-token"
            assert force is False
            return {"status": "ok", "data": {}, "next_steps": []}

    monkeypatch.setattr(cli_module.typer, "prompt", fake_prompt)

    result = runner.invoke(
        app,
        ["install-skill"],
        obj={"install_skill_use_case": FakeInstallSkillUseCase()},
    )

    assert result.exit_code == 0
    assert prompts == ["TUSHARE token"]
```

- [ ] **Step 2: Run test to verify it fails**

Run: `uv run pytest tests/test_cli.py -k "prompts_for_token_when_missing"`
Expected: FAIL because the command does not yet prompt when `--token` is omitted.

- [ ] **Step 3: Implement the CLI wiring**

```python
async def _run_install_skill(
    token: str | None,
    force: bool,
    injected_use_case: object | None,
) -> dict[str, object]:
    use_case = injected_use_case
    if use_case is None:
        use_case = InstallSkillUseCase(
            installer=SkillInstaller(),
            repo_root=Path(__file__).resolve().parent.parent,
        )
    return await use_case.run(token=token, force=force)


@app.command("install-skill")
def install_skill(
    ctx: typer.Context,
    token: str | None = typer.Option(None, "--token"),
    force: bool = typer.Option(False, "--force"),
) -> None:
    effective_token = token if token is not None else typer.prompt("TUSHARE token", hide_input=True)
    payload = asyncio.run(
        _run_install_skill(
            token=effective_token,
            force=force,
            injected_use_case=ctx.obj.get("install_skill_use_case"),
        )
    )
    typer.echo(json.dumps(payload, default=str))
```

- [ ] **Step 4: Add the bootstrap shell script**

Create `scripts/install-global.sh` with this content:

```bash
#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! command -v uv >/dev/null 2>&1; then
  echo "uv is required but was not found in PATH." >&2
  exit 1
fi

exec uv run stock-cache install-skill "$@"
```

- [ ] **Step 5: Run focused verification**

Run: `uv run pytest tests/test_cli.py -k "install_skill"`
Expected: PASS

Run: `uv run stock-cache install-skill --help`
Expected: PASS with `--token` and `--force`

- [ ] **Step 6: Commit**

```bash
git add src/cli.py tests/test_cli.py scripts/install-global.sh
git commit -m "feat: add install-skill cli command"
```

### Task 5: Update README and repository skill docs for standalone installation

**Files:**
- Modify: `README.md`
- Modify: `skills/stock-cache-read/SKILL.md`
- Modify: `skills/stock-cache-write/SKILL.md`

- [ ] **Step 1: Update README with global install usage**

Add a new section with these exact examples:

```markdown
## Global Install And Standalone Skills

Install the tool and the standalone skills from this checkout:

```bash
./scripts/install-global.sh --token YOUR_TUSHARE_TOKEN
```

Or run the install command directly:

```bash
uv run stock-cache install-skill --token YOUR_TUSHARE_TOKEN
```

After install, both runtime forms are supported:

```bash
stock-cache --help
uv tool run stock-cache --help
```

The installed standalone home is:

```text
~/.agents/skills/stock-cache
```
```

- [ ] **Step 2: Rewrite the repository skill docs to match the installed standalone wording**

Required edits:

- remove instructions that require the repository root
- describe the installed standalone home as `~/.agents/skills/stock-cache`
- keep the command examples aligned with the installed global CLI
- keep both skills separate in purpose

- [ ] **Step 3: Run focused verification**

Run: `uv run pytest tests/test_cli.py tests/use_cases/test_install_skill.py tests/services/test_installer.py`
Expected: PASS

Run: `uv run stock-cache --help`
Expected: PASS with `install-skill`, `write`, `read`, `stats`, and `delete`

- [ ] **Step 4: Commit**

```bash
git add README.md skills/stock-cache-read/SKILL.md skills/stock-cache-write/SKILL.md
git commit -m "docs: add standalone install and skill usage"
```

### Task 6: Final verification and cleanup review

**Files:**
- Modify: `src/cli.py`
- Modify: `src/use_cases/install_skill.py`
- Modify: `src/services/installer.py`
- Modify: `src/services/install_templates.py`
- Modify: `tests/test_cli.py`
- Modify: `tests/use_cases/test_install_skill.py`
- Modify: `tests/services/test_installer.py`
- Modify: `README.md`
- Modify: `skills/stock-cache-read/SKILL.md`
- Modify: `skills/stock-cache-write/SKILL.md`
- Create: `scripts/install-global.sh`

- [ ] **Step 1: Run the full targeted verification set**

Run: `uv run pytest tests/test_cli.py tests/use_cases/test_install_skill.py tests/services/test_installer.py -v`
Expected: PASS

- [ ] **Step 2: Run CLI help verification**

Run: `uv run stock-cache --help`
Expected: PASS with `install-skill` listed among the top-level commands.

Run: `uv run stock-cache install-skill --help`
Expected: PASS with `--token` and `--force`.

- [ ] **Step 3: Review the final file set for scope discipline**

Check:

- no new top-level runtime entrypoint besides the Typer command
- no schema file changes
- no read or write JSON contract changes
- no destructive logic against `.runtime/pgsql`

- [ ] **Step 4: Commit**

```bash
git add src/cli.py src/use_cases/install_skill.py src/services/installer.py src/services/install_templates.py tests/test_cli.py tests/use_cases/test_install_skill.py tests/services/test_installer.py README.md skills/stock-cache-read/SKILL.md skills/stock-cache-write/SKILL.md scripts/install-global.sh
git commit -m "feat: add standalone skill installation flow"
```
