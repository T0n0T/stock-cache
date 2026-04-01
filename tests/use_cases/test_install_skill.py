from __future__ import annotations

import asyncio
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
    assert "cd ~/.agents/skills/stock-cache" in call["read_skill_body"]
    assert "stock-cache read raw \\" in call["read_skill_body"]
    assert "stock-cache stats date-range" in call["read_skill_body"]
    assert "cd ~/.agents/skills/stock-cache" in call["write_skill_body"]
    assert "docker compose up -d postgres" in call["write_skill_body"]
    assert "stock-cache --env-file .env config show" in call["write_skill_body"]
    assert "stock-cache --env-file .env init-db" in call["write_skill_body"]
    assert "~/.agents/skills/stock-cache" in call["shared_readme_body"]
    assert "TUSHARE_TOKEN=abc123" in call["env_body"]


def test_install_skill_use_case_rejects_blank_token(tmp_path: Path) -> None:
    installer = FakeInstaller()
    use_case = InstallSkillUseCase(installer=installer, repo_root=tmp_path)

    try:
        asyncio.run(use_case.run(token="   ", force=False))
    except ValueError as exc:
        assert str(exc) == "TUSHARE token is required"
    else:
        raise AssertionError("Expected ValueError for blank token.")
