from dataclasses import dataclass
from pathlib import Path
import subprocess


@dataclass(frozen=True, slots=True)
class InstallerPaths:
    skills_root: Path
    shared_home: Path
    read_skill_dir: Path
    write_skill_dir: Path
    env_file: Path
    compose_file: Path
    runtime_dir: Path
    pgsql_dir: Path
    shared_readme: Path
    cli_marker: Path


def _run_command(args: list[str]) -> None:
    subprocess.run(args, check=True)


class SkillInstaller:
    def __init__(self, home_dir: Path | None = None, run_command: object | None = None) -> None:
        self._home_dir = home_dir or Path.home()
        self._run_command = run_command or _run_command

    def _paths(self) -> InstallerPaths:
        skills_root = self._home_dir / ".agents" / "skills"
        shared_home = skills_root / "stock-cache"
        return InstallerPaths(
            skills_root=skills_root,
            shared_home=shared_home,
            read_skill_dir=skills_root / "stock-cache-read",
            write_skill_dir=skills_root / "stock-cache-write",
            env_file=shared_home / ".env",
            compose_file=shared_home / "compose.yml",
            runtime_dir=shared_home / ".runtime",
            pgsql_dir=shared_home / ".runtime" / "pgsql",
            shared_readme=shared_home / "README.md",
            cli_marker=shared_home / ".installed-cli",
        )

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
        repo_root = repo_root.resolve()
        paths = self._paths()
        self._ensure_directories(paths)
        self._write_compose(repo_root=repo_root, compose_file=paths.compose_file, force=force)
        self._write_text(paths.read_skill_dir / "SKILL.md", read_skill_body, force=force)
        self._write_text(paths.write_skill_dir / "SKILL.md", write_skill_body, force=force)
        self._write_text(paths.shared_readme, shared_readme_body, force=force)
        self._write_env(paths.env_file, env_body, force=force)
        self._install_cli(repo_root=repo_root, marker=paths.cli_marker)
        return {
            "status": "ok",
            "data": {
                "cli_installed": True,
                "cli_command": "stock-cache",
                "shared_home": str(paths.shared_home),
                "skills": [str(paths.read_skill_dir), str(paths.write_skill_dir)],
                "compose_file": str(paths.compose_file),
                "token_written": True,
            },
            "next_steps": [
                "cd ~/.agents/skills/stock-cache",
                "docker compose up -d postgres",
                "stock-cache init-db",
            ],
        }

    def _ensure_directories(self, paths: InstallerPaths) -> None:
        paths.shared_home.mkdir(parents=True, exist_ok=True)
        paths.read_skill_dir.mkdir(parents=True, exist_ok=True)
        paths.write_skill_dir.mkdir(parents=True, exist_ok=True)
        paths.runtime_dir.mkdir(parents=True, exist_ok=True)
        paths.pgsql_dir.mkdir(parents=True, exist_ok=True)

    def _write_compose(self, repo_root: Path, compose_file: Path, force: bool) -> None:
        source = repo_root / "compose.yml"
        contents = source.read_text(encoding="utf-8")
        self._write_text(compose_file, contents, force=force)

    def _write_text(self, path: Path, contents: str, force: bool) -> None:
        if path.exists():
            current = path.read_text(encoding="utf-8")
            if current == contents:
                return
            if not force:
                raise FileExistsError(f"Refusing to overwrite {path} without --force.")
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(contents, encoding="utf-8")

    def _write_env(self, path: Path, contents: str, force: bool) -> None:
        if path.exists():
            current = path.read_text(encoding="utf-8")
            if current == contents:
                return
            if not force and not self._env_differs_only_by_token(current, contents):
                raise FileExistsError(f"Refusing to overwrite {path} without --force.")
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(contents, encoding="utf-8")

    def _env_differs_only_by_token(self, current: str, updated: str) -> bool:
        return self._normalize_env_without_token(current) == self._normalize_env_without_token(updated)

    def _normalize_env_without_token(self, contents: str) -> list[str]:
        lines = contents.splitlines()
        return [
            "__TUSHARE_TOKEN__"
            if line.startswith("TUSHARE_TOKEN=")
            else line
            for line in lines
        ]

    def _install_cli(self, repo_root: Path, marker: Path) -> None:
        args = ["uv", "tool", "install"]
        if marker.exists():
            args.append("--reinstall")
        args.extend(["--from", str(repo_root), "stock-cache"])
        self._run_command(args)
        marker.write_text("installed\n", encoding="utf-8")
