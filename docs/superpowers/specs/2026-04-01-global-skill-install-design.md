# Global Skill Install Design

Date: 2026-04-01

## Overview

This change adds a first-class installation flow that lets users globally install the current `stock-cache` CLI with `uv`, and install two standalone agent skills into `~/.agents/skills` without any runtime dependency on the repository checkout.

The installed result must support both:

- direct global command usage: `stock-cache ...`
- `uv`-managed execution: `uv tool run stock-cache ...`

The two installed skills remain separate:

- `stock-cache-read`
- `stock-cache-write`

They must share one runtime home directory containing `compose.yml`, `.env`, runtime state, and token configuration.

## Goals

- Provide one supported global installation path for the current tool.
- Install real file copies of the two skills into `~/.agents/skills`, not symlinks.
- Copy the repository `compose.yml` into the installed skill home.
- Prompt for a Tushare token at install time when `--token` is not provided.
- Persist the token under the installed skill home so the skills do not depend on the repository directory.
- Keep the CLI thin and keep installation orchestration in use-case and service layers.
- Document the new installation and standalone skill usage in `README.md`.

## Non-Goals

- No new alternate top-level runtime entrypoint besides the existing Typer console script.
- No schema changes.
- No changes to existing read or write JSON output contracts.
- No attempt to auto-start Docker or auto-run `init-db` during installation.
- No destructive cleanup of installed PostgreSQL data directories on reinstall or upgrade.

## User-Facing Flow

The repository will expose two ways to start installation:

### 1. Bootstrap shell script

Users can run:

```bash
./scripts/install-global.sh [--token <TOKEN>] [--force]
```

The script is a thin bootstrap wrapper. It validates that `uv` is available, accepts the same key flags as the CLI command, and invokes the repository-local command that performs the real installation.

### 2. CLI install command

Users can also run:

```bash
uv run stock-cache install-skill [--token <TOKEN>] [--force]
```

This is the authoritative installation path. The shell script exists for convenience, but the real logic lives in Python so it is testable and stays aligned with repository layering rules.

## Recommended Architecture

### CLI layer

`src/cli.py` gains a new `install-skill` command.

Responsibilities:

- parse `--token` and `--force`
- call the install use case
- print a machine-readable JSON result

The CLI must not directly perform path creation, file copying, or subprocess orchestration beyond calling the use case.

### Use-case layer

Add `src/use_cases/install_skill.py`.

Responsibilities:

- receive parsed command inputs
- resolve the repository root and source template paths
- call the installer service in the correct order
- return a structured result object or dictionary for CLI output

This layer owns the installation workflow and policy decisions, but not raw filesystem or subprocess details.

### Service layer

Add:

- `src/services/installer.py`
- `src/services/install_templates.py`

`installer.py` responsibilities:

- ensure target directories exist
- copy skill files and shared files
- write generated config files
- run `uv tool install` or reinstall
- apply overwrite rules

`install_templates.py` responsibilities:

- provide generated `.env` content
- provide generated installed `README.md` content
- provide installed skill content that references the standalone skill home instead of the repository checkout

Keeping templates in a dedicated service module prevents large inline strings from bloating the CLI or use-case modules.

## Installed Directory Layout

Installation writes to the user's home directory under `~/.agents/skills`.

The resulting layout is:

```text
~/.agents/skills/
├── stock-cache/
│   ├── compose.yml
│   ├── .env
│   ├── .runtime/
│   │   ├── pgsql/
│   │   └── last-write-status.txt
│   ├── config/
│   │   └── token.env
│   └── README.md
├── stock-cache-read/
│   └── SKILL.md
└── stock-cache-write/
    └── SKILL.md
```

### Shared home

`~/.agents/skills/stock-cache/` is the shared standalone runtime home.

It contains:

- copied `compose.yml`
- generated `.env`
- runtime status file location
- PostgreSQL data directory
- token configuration
- a small installed README that explains local usage

Both installed skills must instruct the agent to `cd ~/.agents/skills/stock-cache` before running Docker or CLI commands.

### Separate skill entry directories

`stock-cache-read` and `stock-cache-write` stay as separate skills so the agent can invoke them independently by name, matching current repository structure and user intent.

Each installed skill directory contains its own `SKILL.md` as a real copied file, not a symlink.

## Global CLI Installation

The installer must globally install the CLI from the current repository checkout using `uv tool install`.

Preferred behavior:

- first install:

```bash
uv tool install --from <repo-absolute-path> stock-cache
```

- reinstall path when the tool is already present:

```bash
uv tool install --reinstall --from <repo-absolute-path> stock-cache
```

This keeps the existing console script as the only global executable entrypoint while allowing both:

- `stock-cache --help`
- `uv tool run stock-cache --help`

The install flow must not introduce a second top-level executable or wrapper binary for normal runtime usage.

## Token Handling

The install command accepts:

- `--token <TOKEN>`

Behavior:

- if `--token` is provided, use it
- if `--token` is omitted, prompt the user interactively and read the token without echo

The token is stored under:

```text
~/.agents/skills/stock-cache/config/token.env
```

This file is owned by the installed skill home rather than the repository checkout.

Generated `.env` in the shared home must contain the effective runtime configuration for local usage, including:

```dotenv
POSTGRES_DSN=postgresql://postgres:postgres@127.0.0.1:5432/stock_cache
TUSHARE_TOKEN=<token>
STATUS_FILE_PATH=.runtime/last-write-status.txt
```

The installed flow may either duplicate the token into `.env` or generate `.env` from the stored token value at install time. The important requirement is that commands run from `~/.agents/skills/stock-cache` do not need the repository `.env`.

## Overwrite And Reinstall Policy

The installer supports:

- `--force`

Default behavior without `--force`:

- do not overwrite an existing token unless a new `--token` was explicitly provided
- do not delete or recreate `.runtime/pgsql`
- do not remove existing status files
- if copied templates differ and replacement would change installed files, either skip with a clear result message or require `--force`

Behavior with `--force`:

- overwrite installed skill files
- overwrite shared `compose.yml`
- overwrite generated installed README
- overwrite generated `.env`
- preserve `.runtime/pgsql` and other user data directories

The key policy is that force may replace code and template files, but must not destroy the user's cached PostgreSQL data.

## Installed Skill Content

The installed `SKILL.md` files must be rewritten for standalone use.

They must not tell the agent to run from the repository root.

They must instead direct usage through the installed shared home, for example:

```bash
cd ~/.agents/skills/stock-cache
docker compose up -d postgres
stock-cache init-db
stock-cache write --mode full
```

For read commands:

```bash
cd ~/.agents/skills/stock-cache
stock-cache read raw --ts-code 000001.SZ --start-date 2026-01-01 --end-date 2026-03-30
```

The installed skills should continue to explain:

- expected environment state
- database startup through the copied `compose.yml`
- use of the global `stock-cache` command
- relevant `--help` commands

But they must avoid repository-relative assumptions such as `.env` in the repo root or `docker compose` executed from the checkout.

## Bootstrap Script Design

Add `scripts/install-global.sh`.

Responsibilities:

- parse `--token` and `--force`
- check for `uv`
- invoke the authoritative CLI command from the current repository:

```bash
uv run stock-cache install-skill ...
```

The script should stay intentionally small and avoid reimplementing installation policy already present in Python.

## Result Shape

The CLI command should print JSON summarizing installation outcome.

Recommended fields:

```json
{
  "status": "ok",
  "data": {
    "cli_installed": true,
    "cli_command": "stock-cache",
    "shared_home": "/home/example/.agents/skills/stock-cache",
    "skills": [
      "/home/example/.agents/skills/stock-cache-read",
      "/home/example/.agents/skills/stock-cache-write"
    ],
    "compose_file": "/home/example/.agents/skills/stock-cache/compose.yml",
    "token_written": true
  },
  "next_steps": [
    "cd ~/.agents/skills/stock-cache",
    "docker compose up -d postgres",
    "stock-cache init-db"
  ]
}
```

The implementation should keep this shape or a materially equivalent shape that clearly states what was installed and where.

## Testing Strategy

### Unit tests

Add:

- `tests/services/test_installer.py`
- `tests/use_cases/test_install_skill.py`

These tests should cover:

- directory creation for a fresh install
- copying `compose.yml`
- writing skill files as normal files rather than symlinks
- token write behavior with and without a provided replacement token
- `--force` overwrite policy
- preservation of `.runtime/pgsql`
- subprocess invocation for `uv tool install` and reinstall paths

Use temporary directories and mock subprocess execution so tests do not mutate the developer's real `~/.agents` or global `uv` tool environment.

### CLI tests

Extend `tests/test_cli.py` to cover:

- `install-skill --help`
- argument parsing for `--token`
- argument parsing for `--force`
- JSON output shape from the command handler

### Verification commands

Minimum verification for the implementation:

- `uv run pytest tests/test_cli.py tests/use_cases/test_install_skill.py tests/services/test_installer.py`
- `uv run stock-cache --help`
- `uv run stock-cache install-skill --help`

Live verification of actual global `uv tool install` behavior is useful but should not be required in automated tests because it mutates machine-global tool state.

## Documentation Changes

Update `README.md` to add a dedicated installation section covering:

- bootstrap script usage
- direct CLI install usage
- supported runtime forms after install
- installed skill locations
- shared standalone home layout
- token storage behavior
- example startup commands from `~/.agents/skills/stock-cache`

The README should make clear that the installed skills are standalone copies and do not require the original repository directory after installation.

## Risks And Mitigations

### Risk: overwriting user-maintained installed files

Mitigation:

- default to conservative overwrite behavior
- require `--force` for template replacement
- preserve runtime data directories

### Risk: interactive token prompt complicates non-interactive use

Mitigation:

- accept `--token` for automation
- keep interactive prompting only as the fallback path

### Risk: installed skill docs drift from repository skill docs

Mitigation:

- generate installed skill content from template helpers in one place
- keep repository skill docs and installed variants intentionally separate in purpose

### Risk: platform-specific home directory handling

Mitigation:

- use Python home-directory resolution through `Path.home()`
- centralize path construction in installer helpers

## File Boundaries

Create:

- `src/use_cases/install_skill.py`
- `src/services/installer.py`
- `src/services/install_templates.py`
- `scripts/install-global.sh`
- `tests/services/test_installer.py`
- `tests/use_cases/test_install_skill.py`

Modify:

- `src/cli.py`
- `README.md`
- `skills/stock-cache-read/SKILL.md`
- `skills/stock-cache-write/SKILL.md`
- `tests/test_cli.py`

Do not modify:

- database schema files
- existing read or write use-case contracts
- top-level runtime entrypoint structure beyond adding the new CLI subcommand

## Expected Outcome

After this change:

- users can install the current project as a global `uv` tool
- users can run `stock-cache ...` or `uv tool run stock-cache ...`
- both skills are installed as standalone copies under `~/.agents/skills`
- the copied skills use one shared runtime home with a copied `compose.yml`
- the token is stored under the installed skill home
- the installed skill workflow no longer depends on the repository checkout being present
