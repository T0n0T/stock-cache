from pathlib import Path

from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict

DEFAULT_STATUS_FILE_PATH = Path(".runtime/last-write-status.txt")
DEFAULT_INDEX_LIST_PATH = Path(".runtime/default-indexes.csv")


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
    status_file_path: Path = Field(default=DEFAULT_STATUS_FILE_PATH, alias="STATUS_FILE_PATH")
    index_list_path: Path = Field(default=DEFAULT_INDEX_LIST_PATH, alias="INDEX_LIST_PATH")
    allow_indicator_backfill_on_read: bool = Field(default=True, alias="ALLOW_INDICATOR_BACKFILL_ON_READ")
    enable_tushare_indicators: bool = Field(default=True, alias="ENABLE_TUSHARE_INDICATORS")
    enable_local_indicator_fallback: bool = Field(default=True, alias="ENABLE_LOCAL_INDICATOR_FALLBACK")
    write_batch_size: int = Field(default=500, alias="WRITE_BATCH_SIZE")
    log_level: str = Field(default="INFO", alias="LOG_LEVEL")

    model_config = SettingsConfigDict(env_file=".env", env_file_encoding="utf-8", extra="ignore")

    @classmethod
    def from_env_file(cls, env_file: str | Path | None) -> "Settings":
        if env_file is None:
            settings = cls()
            return _normalize_runtime_paths(settings, env_file=None)

        env_path = Path(env_file)
        if not env_path.exists():
            raise FileNotFoundError(f"Env file not found: {env_path}")
        settings = cls(_env_file=env_path)
        return _normalize_runtime_paths(settings, env_file=env_path)


def settings_env_variable_names() -> tuple[str, ...]:
    names: list[str] = []
    for field in Settings.model_fields.values():
        if isinstance(field.alias, str):
            names.append(field.alias)
    return tuple(names)


def _stringify_setting_value(value: object) -> str:
    if isinstance(value, bool):
        return str(value).lower()
    if isinstance(value, Path):
        return value.as_posix()
    return str(value)


def _standalone_home() -> Path:
    return Path.home() / ".agents" / "skills" / "stock-cache"


def _repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def _resolve_explicit_relative_runtime_path(path: Path, env_file: Path | None) -> Path:
    if path.is_absolute():
        return path
    if env_file is not None:
        return (env_file.resolve().parent / path).resolve()
    return path


def _resolve_default_status_file_path(path: Path, env_file: Path | None) -> Path:
    path = _resolve_explicit_relative_runtime_path(path, env_file)
    if path.is_absolute() or env_file is not None or path != DEFAULT_STATUS_FILE_PATH:
        return path
    standalone_home = _standalone_home()
    if standalone_home.exists():
        return (standalone_home / path).resolve()
    return path


def _resolve_default_index_list_path(path: Path, env_file: Path | None) -> Path:
    path = _resolve_explicit_relative_runtime_path(path, env_file)
    if path.is_absolute() or env_file is not None or path != DEFAULT_INDEX_LIST_PATH:
        return path

    standalone_home = _standalone_home()
    if standalone_home.exists():
        return (standalone_home / path).resolve()

    repo_default_path = (_repo_root() / "runtime" / "default-indexes.csv").resolve()
    if repo_default_path.exists():
        return repo_default_path
    return path


def _normalize_runtime_paths(settings: Settings, env_file: Path | None) -> Settings:
    settings.status_file_path = _resolve_default_status_file_path(settings.status_file_path, env_file)
    settings.index_list_path = _resolve_default_index_list_path(settings.index_list_path, env_file)
    return settings


def resolve_runtime_env(
    env_file: str | Path | None,
    variable_names: tuple[str, ...] | None = None,
) -> dict[str, str]:
    if variable_names is None:
        variable_names = settings_env_variable_names()

    settings = Settings.from_env_file(env_file)
    env_to_field_name = {
        field.alias: field_name
        for field_name, field in Settings.model_fields.items()
        if isinstance(field.alias, str)
    }
    resolved: dict[str, str] = {}
    for name in variable_names:
        field_name = env_to_field_name[name]
        resolved[name] = _stringify_setting_value(getattr(settings, field_name))
    return resolved
