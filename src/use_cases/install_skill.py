from __future__ import annotations

from pathlib import Path

from services.install_templates import (
    render_default_indexes_csv,
    render_installed_readme,
    render_installed_read_skill,
    render_installed_write_skill,
    render_shared_env,
)
from services.installer import SkillInstaller


def _require_token(token: str | None) -> str:
    if token is None or token.strip() == "":
        raise ValueError("TUSHARE token is required")
    return token.strip()


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
            default_indexes_csv_body=render_default_indexes_csv(),
        )
