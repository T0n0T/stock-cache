from stock_cache.config import Settings


def test_settings_load_default_values(monkeypatch) -> None:
    monkeypatch.setenv("POSTGRES_DSN", "postgresql://postgres:postgres@localhost:5432/stock_cache")
    monkeypatch.setenv("TUSHARE_TOKEN", "token")

    settings = Settings()

    assert settings.default_lookback_trading_days == 90
    assert settings.status_file_path.as_posix() == "runtime/last-write-status.txt"
    assert settings.max_retries == 3
