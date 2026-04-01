from pathlib import Path

from services.install_templates import (
    render_installed_read_skill,
    render_installed_write_skill,
)


def _repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def test_render_installed_read_skill_matches_repository_skill_doc() -> None:
    expected = (_repo_root() / "skills" / "stock-cache-read" / "SKILL.md").read_text(
        encoding="utf-8"
    )

    assert render_installed_read_skill() == expected


def test_render_installed_write_skill_matches_repository_skill_doc() -> None:
    expected = (_repo_root() / "skills" / "stock-cache-write" / "SKILL.md").read_text(
        encoding="utf-8"
    )

    assert render_installed_write_skill() == expected
