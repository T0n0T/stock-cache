from pathlib import Path

import pytest

from services.installer import InstallerPaths, SkillInstaller


class RecordingRunner:
    def __init__(self) -> None:
        self.calls: list[list[str]] = []

    def __call__(self, args: list[str]) -> None:
        self.calls.append(args)


def _create_repo_root(tmp_path: Path) -> Path:
    repo_root = tmp_path / "repo"
    repo_root.mkdir()
    (repo_root / "compose.yml").write_text("services:\n  postgres:\n    image: postgres:alpine\n", encoding="utf-8")
    return repo_root


def test_installer_paths_build_expected_layout(tmp_path: Path) -> None:
    installer = SkillInstaller(home_dir=tmp_path)

    paths = installer._paths()

    assert paths == InstallerPaths(
        skills_root=tmp_path / ".agents" / "skills",
        shared_home=tmp_path / ".agents" / "skills" / "stock-cache",
        read_skill_dir=tmp_path / ".agents" / "skills" / "stock-cache-read",
        write_skill_dir=tmp_path / ".agents" / "skills" / "stock-cache-write",
        env_file=tmp_path / ".agents" / "skills" / "stock-cache" / ".env",
        compose_file=tmp_path / ".agents" / "skills" / "stock-cache" / "compose.yml",
        runtime_dir=tmp_path / ".agents" / "skills" / "stock-cache" / ".runtime",
        pgsql_dir=tmp_path / ".agents" / "skills" / "stock-cache" / ".runtime" / "pgsql",
        default_indexes_file=tmp_path / ".agents" / "skills" / "stock-cache" / ".runtime" / "default-indexes.csv",
        shared_readme=tmp_path / ".agents" / "skills" / "stock-cache" / "README.md",
        cli_marker=tmp_path / ".agents" / "skills" / "stock-cache" / ".installed-cli",
    )


def test_installer_creates_shared_home_and_skill_copies(tmp_path: Path) -> None:
    repo_root = _create_repo_root(tmp_path)
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
        env_body=(
            "POSTGRES_DSN=postgresql://postgres:postgres@127.0.0.1:5432/stock_cache\n"
            "TUSHARE_TOKEN=token-1\n"
            "STATUS_FILE_PATH=.runtime/last-write-status.txt\n"
            "INDEX_LIST_PATH=.runtime/default-indexes.csv\n"
        ),
        default_indexes_csv_body="ts_code,name,group_name,enabled\n000001.SH,上证指数,major,true\n",
    )

    assert (home_dir / ".agents" / "skills" / "stock-cache" / "compose.yml").exists()
    assert (
        home_dir / ".agents" / "skills" / "stock-cache" / ".env"
    ).read_text(encoding="utf-8").splitlines() == [
        "POSTGRES_DSN=postgresql://postgres:postgres@127.0.0.1:5432/stock_cache",
        "TUSHARE_TOKEN=token-1",
        "STATUS_FILE_PATH=.runtime/last-write-status.txt",
        "INDEX_LIST_PATH=.runtime/default-indexes.csv",
    ]
    assert (
        home_dir / ".agents" / "skills" / "stock-cache" / ".runtime" / "default-indexes.csv"
    ).read_text(encoding="utf-8") == "ts_code,name,group_name,enabled\n000001.SH,上证指数,major,true\n"
    assert not (home_dir / ".agents" / "skills" / "stock-cache" / "config").exists()
    assert (home_dir / ".agents" / "skills" / "stock-cache-read" / "SKILL.md").is_symlink() is False
    assert (
        home_dir / ".agents" / "skills" / "stock-cache-read" / "SKILL.md"
    ).read_text(encoding="utf-8") == "# read skill\n"
    assert (home_dir / ".agents" / "skills" / "stock-cache-write" / "SKILL.md").is_symlink() is False
    assert (
        home_dir / ".agents" / "skills" / "stock-cache-write" / "SKILL.md"
    ).read_text(encoding="utf-8") == "# write skill\n"
    assert runner.calls == [["uv", "tool", "install", "--from", str(repo_root), "stock-cache"]]
    assert result["data"]["token_written"] is True
    assert result["data"]["shared_home"] == str(home_dir / ".agents" / "skills" / "stock-cache")
    assert result["data"]["skills"] == [
        str(home_dir / ".agents" / "skills" / "stock-cache-read"),
        str(home_dir / ".agents" / "skills" / "stock-cache-write"),
    ]
    assert result["data"]["compose_file"] == str(home_dir / ".agents" / "skills" / "stock-cache" / "compose.yml")
    assert result["data"]["default_indexes_file"] == str(
        home_dir / ".agents" / "skills" / "stock-cache" / ".runtime" / "default-indexes.csv"
    )
    assert result["next_steps"] == [
        "cd ~/.agents/skills/stock-cache",
        "docker compose up -d postgres",
        "stock-cache init-db",
    ]


def test_installer_requires_force_before_overwriting_existing_templates(tmp_path: Path) -> None:
    repo_root = _create_repo_root(tmp_path)
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
        env_body=(
            "POSTGRES_DSN=postgresql://postgres:postgres@127.0.0.1:5432/stock_cache\n"
            "TUSHARE_TOKEN=token-1\n"
            "STATUS_FILE_PATH=.runtime/last-write-status.txt\n"
            "INDEX_LIST_PATH=.runtime/default-indexes.csv\n"
        ),
        default_indexes_csv_body="one\n",
    )

    with pytest.raises(FileExistsError):
        installer.install(
            repo_root=repo_root,
            token="token-1",
            force=False,
            read_skill_body="# read skill v2\n",
            write_skill_body="# write skill v2\n",
            shared_readme_body="# local readme v2\n",
            env_body=(
                "POSTGRES_DSN=postgresql://postgres:postgres@127.0.0.1:5432/stock_cache\n"
                "TUSHARE_TOKEN=token-1\n"
                "STATUS_FILE_PATH=.runtime/last-write-status.txt\n"
                "INDEX_LIST_PATH=.runtime/default-indexes.csv\n"
            ),
            default_indexes_csv_body="two\n",
        )


def test_installer_reinstalls_when_cli_already_present(tmp_path: Path) -> None:
    repo_root = _create_repo_root(tmp_path)
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
        env_body=(
            "POSTGRES_DSN=postgresql://postgres:postgres@127.0.0.1:5432/stock_cache\n"
            "TUSHARE_TOKEN=token-1\n"
            "STATUS_FILE_PATH=.runtime/last-write-status.txt\n"
            "INDEX_LIST_PATH=.runtime/default-indexes.csv\n"
        ),
        default_indexes_csv_body="one\n",
    )

    assert runner.calls == [["uv", "tool", "install", "--reinstall", "--from", str(repo_root), "stock-cache"]]


def test_installer_normalizes_repo_root_to_absolute_path_for_uv_install(tmp_path: Path, monkeypatch) -> None:
    repo_root = _create_repo_root(tmp_path)
    home_dir = tmp_path / "home"
    runner = RecordingRunner()
    installer = SkillInstaller(home_dir=home_dir, run_command=runner)

    monkeypatch.chdir(tmp_path)
    installer.install(
        repo_root=Path("repo"),
        token="token-1",
        force=False,
        read_skill_body="# read skill\n",
        write_skill_body="# write skill\n",
        shared_readme_body="# local readme\n",
        env_body=(
            "POSTGRES_DSN=postgresql://postgres:postgres@127.0.0.1:5432/stock_cache\n"
            "TUSHARE_TOKEN=token-1\n"
            "STATUS_FILE_PATH=.runtime/last-write-status.txt\n"
            "INDEX_LIST_PATH=.runtime/default-indexes.csv\n"
        ),
        default_indexes_csv_body="one\n",
    )

    assert runner.calls == [["uv", "tool", "install", "--from", str(repo_root.resolve()), "stock-cache"]]


def test_installer_preserves_runtime_pgsql_directory_when_forcing(tmp_path: Path) -> None:
    repo_root = _create_repo_root(tmp_path)
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
        env_body=(
            "POSTGRES_DSN=postgresql://postgres:postgres@127.0.0.1:5432/stock_cache\n"
            "TUSHARE_TOKEN=token-1\n"
            "STATUS_FILE_PATH=.runtime/last-write-status.txt\n"
            "INDEX_LIST_PATH=.runtime/default-indexes.csv\n"
        ),
        default_indexes_csv_body="one\n",
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
        env_body=(
            "POSTGRES_DSN=postgresql://postgres:postgres@127.0.0.1:5432/stock_cache\n"
            "TUSHARE_TOKEN=token-2\n"
            "STATUS_FILE_PATH=.runtime/last-write-status.txt\n"
            "INDEX_LIST_PATH=.runtime/default-indexes.csv\n"
        ),
        default_indexes_csv_body="two\n",
    )

    assert pgsql_file.read_text(encoding="utf-8") == "17\n"
    assert not (home_dir / ".agents" / "skills" / "stock-cache" / "config").exists()
    assert (
        home_dir / ".agents" / "skills" / "stock-cache" / ".env"
    ).read_text(encoding="utf-8").splitlines() == [
        "POSTGRES_DSN=postgresql://postgres:postgres@127.0.0.1:5432/stock_cache",
        "TUSHARE_TOKEN=token-2",
        "STATUS_FILE_PATH=.runtime/last-write-status.txt",
        "INDEX_LIST_PATH=.runtime/default-indexes.csv",
    ]
    assert (
        home_dir / ".agents" / "skills" / "stock-cache" / ".runtime" / "default-indexes.csv"
    ).read_text(encoding="utf-8") == "two\n"


def test_installer_allows_token_rotation_without_force_when_only_env_token_changes(tmp_path: Path) -> None:
    repo_root = _create_repo_root(tmp_path)
    home_dir = tmp_path / "home"
    runner = RecordingRunner()
    installer = SkillInstaller(home_dir=home_dir, run_command=runner)

    installer.install(
        repo_root=repo_root,
        token="token-1",
        force=False,
        read_skill_body="# read skill\n",
        write_skill_body="# write skill\n",
        shared_readme_body="# local readme\n",
        env_body=(
            "POSTGRES_DSN=postgresql://postgres:postgres@127.0.0.1:5432/stock_cache\n"
            "TUSHARE_TOKEN=token-1\n"
            "STATUS_FILE_PATH=.runtime/last-write-status.txt\n"
            "INDEX_LIST_PATH=.runtime/default-indexes.csv\n"
        ),
        default_indexes_csv_body="one\n",
    )

    installer.install(
        repo_root=repo_root,
        token="token-2",
        force=False,
        read_skill_body="# read skill\n",
        write_skill_body="# write skill\n",
        shared_readme_body="# local readme\n",
        env_body=(
            "POSTGRES_DSN=postgresql://postgres:postgres@127.0.0.1:5432/stock_cache\n"
            "TUSHARE_TOKEN=token-2\n"
            "STATUS_FILE_PATH=.runtime/last-write-status.txt\n"
            "INDEX_LIST_PATH=.runtime/default-indexes.csv\n"
        ),
        default_indexes_csv_body="one\n",
    )

    assert not (home_dir / ".agents" / "skills" / "stock-cache" / "config").exists()
    assert (
        home_dir / ".agents" / "skills" / "stock-cache" / ".env"
    ).read_text(encoding="utf-8").splitlines() == [
        "POSTGRES_DSN=postgresql://postgres:postgres@127.0.0.1:5432/stock_cache",
        "TUSHARE_TOKEN=token-2",
        "STATUS_FILE_PATH=.runtime/last-write-status.txt",
        "INDEX_LIST_PATH=.runtime/default-indexes.csv",
    ]
