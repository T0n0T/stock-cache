from pathlib import Path

from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    postgres_dsn: str = Field(alias="POSTGRES_DSN")
    tushare_token: str = Field(alias="TUSHARE_TOKEN")
    max_concurrency: int = Field(default=20, alias="MAX_CONCURRENCY")
    max_retries: int = Field(default=3, alias="MAX_RETRIES")
    retry_base_delay: float = Field(default=1.0, alias="RETRY_BASE_DELAY")
    retry_backoff_factor: float = Field(default=2.0, alias="RETRY_BACKOFF_FACTOR")
    retry_jitter: float = Field(default=0.2, alias="RETRY_JITTER")
    request_timeout_seconds: int = Field(default=20, alias="REQUEST_TIMEOUT_SECONDS")
    default_lookback_trading_days: int = Field(default=90, alias="DEFAULT_LOOKBACK_TRADING_DAYS")
    status_file_path: Path = Field(default=Path(".runtime/last-write-status.txt"), alias="STATUS_FILE_PATH")
    allow_indicator_backfill_on_read: bool = Field(default=True, alias="ALLOW_INDICATOR_BACKFILL_ON_READ")
    enable_tushare_indicators: bool = Field(default=True, alias="ENABLE_TUSHARE_INDICATORS")
    enable_local_indicator_fallback: bool = Field(default=True, alias="ENABLE_LOCAL_INDICATOR_FALLBACK")
    write_batch_size: int = Field(default=500, alias="WRITE_BATCH_SIZE")
    log_level: str = Field(default="INFO", alias="LOG_LEVEL")

    model_config = SettingsConfigDict(env_file=".env", env_file_encoding="utf-8", extra="ignore")
