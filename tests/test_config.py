import logging
from pathlib import Path

import pytest

from app_logging import configure_logging
from config import Settings, resolve_runtime_env, settings_env_variable_names


def test_settings_load_default_values(monkeypatch, sample_dsn: str) -> None:
    monkeypatch.setenv("POSTGRES_DSN", sample_dsn)
    monkeypatch.setenv("TUSHARE_TOKEN", "token")

    settings = Settings()

    assert settings.default_lookback_trading_days == 90
    assert settings.status_file_path.as_posix() == ".runtime/last-write-status.txt"
    assert settings.index_list_path.as_posix() == "runtime/default-indexes.csv"
    assert settings.max_retries == 3


def test_settings_coerces_env_alias_types(monkeypatch, sample_dsn: str) -> None:
    monkeypatch.setenv("POSTGRES_DSN", sample_dsn)
    monkeypatch.setenv("TUSHARE_TOKEN", "token")
    monkeypatch.setenv("MAX_RETRIES", "5")
    monkeypatch.setenv("ALLOW_INDICATOR_BACKFILL_ON_READ", "false")
    monkeypatch.setenv("STATUS_FILE_PATH", "runtime/custom-status.txt")

    settings = Settings()

    assert settings.max_retries == 5
    assert settings.allow_indicator_backfill_on_read is False
    assert settings.status_file_path == Path("runtime/custom-status.txt")


def test_configure_logging_forces_level_with_existing_root_handler() -> None:
    root_logger = logging.getLogger()
    original_handlers = root_logger.handlers[:]
    original_level = root_logger.level

    try:
        root_logger.handlers.clear()
        root_logger.addHandler(logging.StreamHandler())
        root_logger.setLevel(logging.WARNING)

        configure_logging("debug")

        assert root_logger.level == logging.DEBUG
    finally:
        root_logger.handlers.clear()
        root_logger.handlers.extend(original_handlers)
        root_logger.setLevel(original_level)


def test_settings_loads_values_from_explicit_env_file(monkeypatch, tmp_path: Path) -> None:
    monkeypatch.delenv("POSTGRES_DSN", raising=False)
    monkeypatch.delenv("TUSHARE_TOKEN", raising=False)
    env_file = tmp_path / "custom.env"
    env_file.write_text(
        "\n".join(
            [
                "POSTGRES_DSN=postgresql://file:file@127.0.0.1:5432/file_db",
                "TUSHARE_TOKEN=file-token",
            ]
        )
        + "\n",
        encoding="utf-8",
    )

    settings = Settings.from_env_file(env_file)

    assert settings.postgres_dsn == "postgresql://file:file@127.0.0.1:5432/file_db"
    assert settings.tushare_token == "file-token"


def test_settings_prefers_shell_environment_over_explicit_env_file(monkeypatch, tmp_path: Path) -> None:
    env_file = tmp_path / "custom.env"
    env_file.write_text(
        "\n".join(
            [
                "POSTGRES_DSN=postgresql://file:file@127.0.0.1:5432/file_db",
                "TUSHARE_TOKEN=file-token",
            ]
        )
        + "\n",
        encoding="utf-8",
    )
    monkeypatch.setenv("POSTGRES_DSN", "postgresql://env:env@127.0.0.1:5432/env_db")
    monkeypatch.setenv("TUSHARE_TOKEN", "env-token")

    settings = Settings.from_env_file(env_file)

    assert settings.postgres_dsn == "postgresql://env:env@127.0.0.1:5432/env_db"
    assert settings.tushare_token == "env-token"


def test_settings_rejects_missing_explicit_env_file(tmp_path: Path) -> None:
    missing_file = tmp_path / "missing.env"

    with pytest.raises(FileNotFoundError):
        Settings.from_env_file(missing_file)


def test_settings_env_variable_names_include_all_declared_fields() -> None:
    assert settings_env_variable_names() == (
        "POSTGRES_DSN",
        "TUSHARE_TOKEN",
        "MAX_CONCURRENCY",
        "MAX_RETRIES",
        "RETRY_BASE_DELAY",
        "RETRY_BACKOFF_FACTOR",
        "RETRY_JITTER",
        "REQUEST_TIMEOUT_SECONDS",
        "DEFAULT_LOOKBACK_TRADING_DAYS",
        "STATUS_FILE_PATH",
        "INDEX_LIST_PATH",
        "ALLOW_INDICATOR_BACKFILL_ON_READ",
        "ENABLE_TUSHARE_INDICATORS",
        "ENABLE_LOCAL_INDICATOR_FALLBACK",
        "WRITE_BATCH_SIZE",
        "LOG_LEVEL",
    )


def test_resolve_runtime_env_returns_all_effective_values(monkeypatch, tmp_path: Path) -> None:
    monkeypatch.delenv("POSTGRES_DSN", raising=False)
    monkeypatch.delenv("TUSHARE_TOKEN", raising=False)
    monkeypatch.delenv("MAX_CONCURRENCY", raising=False)
    monkeypatch.delenv("ALLOW_INDICATOR_BACKFILL_ON_READ", raising=False)
    env_file = tmp_path / "custom.env"
    env_file.write_text(
        "\n".join(
            [
                "POSTGRES_DSN=postgresql://file:file@127.0.0.1:5432/file_db",
                "TUSHARE_TOKEN=file-token",
                "MAX_CONCURRENCY=11",
                "ALLOW_INDICATOR_BACKFILL_ON_READ=false",
            ]
        )
        + "\n",
        encoding="utf-8",
    )

    values = resolve_runtime_env(env_file)

    assert values["POSTGRES_DSN"] == "postgresql://file:file@127.0.0.1:5432/file_db"
    assert values["TUSHARE_TOKEN"] == "file-token"
    assert values["MAX_CONCURRENCY"] == "11"
    assert values["ALLOW_INDICATOR_BACKFILL_ON_READ"] == "false"
    assert values["STATUS_FILE_PATH"] == ".runtime/last-write-status.txt"
    assert values["INDEX_LIST_PATH"] == "runtime/default-indexes.csv"


def test_resolve_runtime_env_stringifies_parsed_shell_values(monkeypatch, sample_dsn: str) -> None:
    monkeypatch.setenv("POSTGRES_DSN", sample_dsn)
    monkeypatch.setenv("TUSHARE_TOKEN", "token")
    monkeypatch.setenv("ALLOW_INDICATOR_BACKFILL_ON_READ", "0")

    values = resolve_runtime_env(None)

    assert values["ALLOW_INDICATOR_BACKFILL_ON_READ"] == "false"
