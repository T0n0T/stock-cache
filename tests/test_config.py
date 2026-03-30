import logging
from pathlib import Path

from stock_cache.config import Settings
from stock_cache.logging import configure_logging


def test_settings_load_default_values(monkeypatch, sample_dsn: str) -> None:
    monkeypatch.setenv("POSTGRES_DSN", sample_dsn)
    monkeypatch.setenv("TUSHARE_TOKEN", "token")

    settings = Settings()

    assert settings.default_lookback_trading_days == 90
    assert settings.status_file_path.as_posix() == "runtime/last-write-status.txt"
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
