from pathlib import Path
import tomllib


def test_direct_cli_imports_are_declared_runtime_dependencies() -> None:
    pyproject = tomllib.loads(Path("pyproject.toml").read_text(encoding="utf-8"))
    dependencies = {
        dependency.split("[", 1)[0].split(">", 1)[0].split("=", 1)[0].split("<", 1)[0].strip()
        for dependency in pyproject["project"]["dependencies"]
    }

    assert "click" in dependencies

