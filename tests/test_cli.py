import json
import time
from dataclasses import dataclass
from datetime import date
from pathlib import Path
from requests import ConnectionError as RequestsConnectionError

import pytest
from typer.testing import CliRunner

import cli as cli_module
from cli import app
from db.pool import DatabasePrecheckError
from domain.models import DailyIndicatorRow, DailyMarketRow, Instrument
from providers.tushare_adapter import TushareAdapter
from use_cases.read_raw import ReadRawMarketDataUseCase
from use_cases.read_screen import ReadScreeningResultsUseCase


runner = CliRunner()


class FakeInstallSkillUseCase:
    def __init__(self) -> None:
        self.calls: list[dict[str, object]] = []

    async def run(self, token: str | None, force: bool) -> dict[str, object]:
        self.calls.append({"token": token, "force": force})
        return {
            "status": "ok",
            "data": {
                "cli_installed": True,
                "cli_command": "stock-cache",
                "shared_home": "/home/example/.agents/skills/stock-cache",
                "skills": [
                    "/home/example/.agents/skills/stock-cache-read",
                    "/home/example/.agents/skills/stock-cache-write",
                ],
                "compose_file": "/home/example/.agents/skills/stock-cache/compose.yml",
                "default_indexes_file": "/home/example/.agents/skills/stock-cache/.runtime/default-indexes.csv",
                "token_written": True,
            },
            "next_steps": [
                "cd ~/.agents/skills/stock-cache",
                "docker compose up -d postgres",
                "stock-cache init-db",
            ],
        }


def test_cli_help_lists_install_skill_command() -> None:
    result = runner.invoke(app, ["--help"])

    assert result.exit_code == 0
    assert "install-skill" in result.stdout


def test_cli_help_lists_config_command_and_global_env_file_option() -> None:
    result = runner.invoke(app, ["--help"])

    assert result.exit_code == 0
    assert "config" in result.stdout
    assert "--env-file" in result.stdout


def test_cli_config_help_lists_show_subcommand() -> None:
    result = runner.invoke(app, ["config", "--help"])

    assert result.exit_code == 0
    assert "show" in result.stdout


def test_cli_config_show_prints_values_from_explicit_env_file(monkeypatch, tmp_path: Path) -> None:
    monkeypatch.delenv("POSTGRES_DSN", raising=False)
    monkeypatch.delenv("TUSHARE_TOKEN", raising=False)
    monkeypatch.delenv("MAX_CONCURRENCY", raising=False)
    monkeypatch.delenv("MAX_RETRIES", raising=False)
    monkeypatch.delenv("RETRY_BASE_DELAY", raising=False)
    monkeypatch.delenv("RETRY_BACKOFF_FACTOR", raising=False)
    monkeypatch.delenv("RETRY_JITTER", raising=False)
    monkeypatch.delenv("REQUEST_TIMEOUT_SECONDS", raising=False)
    monkeypatch.delenv("DEFAULT_LOOKBACK_TRADING_DAYS", raising=False)
    monkeypatch.delenv("STATUS_FILE_PATH", raising=False)
    monkeypatch.delenv("ALLOW_INDICATOR_BACKFILL_ON_READ", raising=False)
    monkeypatch.delenv("ENABLE_TUSHARE_INDICATORS", raising=False)
    monkeypatch.delenv("ENABLE_LOCAL_INDICATOR_FALLBACK", raising=False)
    monkeypatch.delenv("WRITE_BATCH_SIZE", raising=False)
    monkeypatch.delenv("LOG_LEVEL", raising=False)
    env_file = tmp_path / "custom.env"
    env_file.write_text(
        "\n".join(
            [
                "POSTGRES_DSN=postgresql://file:file@127.0.0.1:5432/file_db",
                "TUSHARE_TOKEN=file-token",
                "MAX_CONCURRENCY=11",
                "MAX_RETRIES=5",
                "RETRY_BASE_DELAY=0.5",
                "RETRY_BACKOFF_FACTOR=1.5",
                "RETRY_JITTER=0.1",
                "REQUEST_TIMEOUT_SECONDS=12",
                "DEFAULT_LOOKBACK_TRADING_DAYS=30",
                "STATUS_FILE_PATH=runtime/custom-status.txt",
                "INDEX_LIST_PATH=runtime/custom-indexes.csv",
                "ALLOW_INDICATOR_BACKFILL_ON_READ=false",
                "ENABLE_TUSHARE_INDICATORS=false",
                "ENABLE_LOCAL_INDICATOR_FALLBACK=false",
                "WRITE_BATCH_SIZE=250",
                "LOG_LEVEL=DEBUG",
            ]
        )
        + "\n",
        encoding="utf-8",
    )

    result = runner.invoke(app, ["--env-file", str(env_file), "config", "show"])

    assert result.exit_code == 0
    assert result.stdout == (
        "POSTGRES_DSN=postgresql://file:file@127.0.0.1:5432/file_db\n"
        "TUSHARE_TOKEN=file-token\n"
        "MAX_CONCURRENCY=11\n"
        "MAX_RETRIES=5\n"
        "RETRY_BASE_DELAY=0.5\n"
        "RETRY_BACKOFF_FACTOR=1.5\n"
        "RETRY_JITTER=0.1\n"
        "REQUEST_TIMEOUT_SECONDS=12\n"
        "DEFAULT_LOOKBACK_TRADING_DAYS=30\n"
        "STATUS_FILE_PATH=runtime/custom-status.txt\n"
        "INDEX_LIST_PATH=runtime/custom-indexes.csv\n"
        "ALLOW_INDICATOR_BACKFILL_ON_READ=false\n"
        "ENABLE_TUSHARE_INDICATORS=false\n"
        "ENABLE_LOCAL_INDICATOR_FALLBACK=false\n"
        "WRITE_BATCH_SIZE=250\n"
        "LOG_LEVEL=DEBUG\n"
    )


def test_cli_config_show_prefers_shell_environment_over_explicit_env_file(monkeypatch, tmp_path: Path) -> None:
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
    monkeypatch.setenv("POSTGRES_DSN", "postgresql://env:env@127.0.0.1:5432/env_db")
    monkeypatch.setenv("TUSHARE_TOKEN", "env-token")
    monkeypatch.setenv("MAX_CONCURRENCY", "99")
    monkeypatch.setenv("ALLOW_INDICATOR_BACKFILL_ON_READ", "true")

    result = runner.invoke(app, ["--env-file", str(env_file), "config", "show"])

    assert result.exit_code == 0
    assert "POSTGRES_DSN=postgresql://env:env@127.0.0.1:5432/env_db\n" in result.stdout
    assert "TUSHARE_TOKEN=env-token\n" in result.stdout
    assert "MAX_CONCURRENCY=99\n" in result.stdout
    assert "ALLOW_INDICATOR_BACKFILL_ON_READ=true\n" in result.stdout


def test_cli_config_show_rejects_missing_explicit_env_file(tmp_path: Path) -> None:
    missing_file = tmp_path / "missing.env"

    result = runner.invoke(app, ["--env-file", str(missing_file), "config", "show"])

    assert result.exit_code == 2
    assert "Env file not found" in result.output
    assert missing_file.name in result.output


def test_cli_init_db_uses_explicit_env_file_for_runtime_settings(monkeypatch, tmp_path: Path) -> None:
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
    calls: dict[str, object] = {}

    async def fake_create_pool(dsn: str) -> object:
        calls["dsn"] = dsn

        class FakePool:
            async def close(self) -> None:
                calls["closed"] = True

        return FakePool()

    async def fake_initialize_schema(pool: object) -> dict[str, object]:
        calls["pool"] = pool
        return {"status": "ok", "created_tables": [], "already_present": [], "missing": []}

    monkeypatch.setattr("cli.create_pool", fake_create_pool)
    monkeypatch.setattr("cli.initialize_schema", fake_initialize_schema)

    result = runner.invoke(app, ["--env-file", str(env_file), "init-db"])

    assert result.exit_code == 0
    assert calls["dsn"] == "postgresql://file:file@127.0.0.1:5432/file_db"
    assert calls["closed"] is True


def test_cli_install_skill_help_lists_token_and_force() -> None:
    result = runner.invoke(app, ["install-skill", "--help"])

    assert result.exit_code == 0
    assert "--token" in result.stdout
    assert "--force" in result.stdout


def test_cli_install_skill_passes_flags_to_use_case() -> None:
    use_case = FakeInstallSkillUseCase()

    result = runner.invoke(
        app,
        ["install-skill", "--token", "abc123", "--force"],
        obj={"install_skill_use_case": use_case},
    )

    assert result.exit_code == 0
    assert use_case.calls == [{"token": "abc123", "force": True}]
    assert "Installed stock-cache global CLI and standalone skills." in result.stdout
    assert "Shared home: /home/example/.agents/skills/stock-cache" in result.stdout
    assert "Installed skills:" in result.stdout
    assert "/home/example/.agents/skills/stock-cache-read" in result.stdout
    assert "/home/example/.agents/skills/stock-cache-write" in result.stdout
    assert "Compose file: /home/example/.agents/skills/stock-cache/compose.yml" in result.stdout
    assert "Default index list: /home/example/.agents/skills/stock-cache/.runtime/default-indexes.csv" in result.stdout
    assert "Next steps:" in result.stdout
    assert "cd ~/.agents/skills/stock-cache" in result.stdout
    assert "docker compose up -d postgres" in result.stdout
    assert "stock-cache init-db" in result.stdout


def test_cli_install_skill_prompts_for_token_when_missing(monkeypatch) -> None:
    prompts: list[str] = []

    def fake_prompt(text: str, hide_input: bool = False) -> str:
        prompts.append(text)
        assert hide_input is True
        return "prompt-token"

    class TokenCase(FakeInstallSkillUseCase):
        async def run(self, token: str | None, force: bool) -> dict[str, object]:
            assert token == "prompt-token"
            assert force is False
            return await super().run(token=token, force=force)

    monkeypatch.setattr(cli_module.typer, "prompt", fake_prompt)

    result = runner.invoke(
        app,
        ["install-skill"],
        obj={"install_skill_use_case": TokenCase()},
    )

    assert result.exit_code == 0
    assert prompts == ["TUSHARE token"]


def test_cli_install_skill_reports_plain_error_for_blank_token() -> None:
    result = runner.invoke(app, ["install-skill", "--token", ""])

    assert result.exit_code == 1
    assert result.stdout == "Error: TUSHARE token is required\n"



def test_cli_help_lists_write_and_read_commands() -> None:
    result = runner.invoke(app, ["--help"])

    assert result.exit_code == 0
    assert "write" in result.stdout
    assert "read" in result.stdout
    assert "stats" in result.stdout
    assert "delete" in result.stdout


def test_read_help_lists_raw_and_screen_subcommands() -> None:
    result = runner.invoke(app, ["read", "--help"])

    assert result.exit_code == 0
    assert "raw" in result.stdout
    assert "screen" in result.stdout


def test_stats_help_lists_date_range_subcommand() -> None:
    result = runner.invoke(app, ["stats", "--help"])

    assert result.exit_code == 0
    assert "date-range" in result.stdout


def test_delete_help_lists_by_date_subcommand() -> None:
    result = runner.invoke(app, ["delete", "--help"])

    assert result.exit_code == 0
    assert "by-date" in result.stdout


def test_write_help_describes_supported_modes_and_single_selectors() -> None:
    result = runner.invoke(app, ["write", "--help"])

    assert result.exit_code == 0
    assert "full" in result.stdout
    assert "single" in result.stdout
    assert "failed-only" not in result.stdout
    assert "--ts-code" in result.stdout
    assert "--name" in result.stdout


class FakeMarketRepository:
    async def fetch_raw(self, ts_code: str, start_date: str, end_date: str) -> dict[str, list[object]]:
        return {
            "market": [DailyMarketRow(ts_code=ts_code, trade_date=date(2026, 3, 30), close=12.4)],
            "indicators": [DailyIndicatorRow(ts_code=ts_code, trade_date=date(2026, 3, 30), macd=0.1)],
        }


def test_cli_read_raw_prints_json_payload() -> None:
    result = runner.invoke(
        app,
        ["read", "raw", "--ts-code", "000001.SZ", "--start-date", "2026-01-01", "--end-date", "2026-03-30"],
        obj={"read_raw_use_case": ReadRawMarketDataUseCase(FakeMarketRepository())},
    )

    assert result.exit_code == 0
    payload = json.loads(result.stdout)
    assert payload["query"]["ts_code"] == "000001.SZ"
    assert payload["meta"]["row_count_market"] == 1
    assert payload["meta"]["row_count_indicators"] == 1


def test_cli_read_raw_requires_exactly_one_lookup_selector() -> None:
    result = runner.invoke(
        app,
        [
            "read",
            "raw",
            "--ts-code",
            "000001.SZ",
            "--name",
            "Ping An",
            "--start-date",
            "2026-01-01",
            "--end-date",
            "2026-03-30",
        ],
    )

    assert result.exit_code == 2
    assert "Exactly one of --ts-code or --name must be provided." in result.output


def test_cli_read_raw_reports_postgres_precheck_failure(monkeypatch) -> None:
    async def fake_run_read_raw(
        ts_code: str | None,
        name: str | None,
        start_date: str,
        end_date: str,
        injected_use_case: object | None,
    ) -> dict[str, object]:
        _ = (ts_code, name, start_date, end_date, injected_use_case)
        raise DatabasePrecheckError("PostgreSQL is not reachable at configured POSTGRES_DSN.")

    monkeypatch.setattr("cli._run_read_raw", fake_run_read_raw, raising=False)

    result = runner.invoke(
        app,
        ["read", "raw", "--ts-code", "000001.SZ", "--start-date", "2026-01-01", "--end-date", "2026-03-30"],
    )

    assert result.exit_code == 1
    payload = json.loads(result.stdout)
    assert payload == {
        "status": "error",
        "error": "postgres_unreachable",
        "message": "PostgreSQL is not reachable at configured POSTGRES_DSN.",
    }


class FakeScreenRepository:
    async def screen(self, trade_date: str, filters: dict[str, object]) -> list[dict[str, object]]:
        return [
            {
                "ts_code": "300001.SZ",
                "trade_date": trade_date,
                "pct_chg": filters["pct_chg_gte"],
                "turnover_rate": filters["turnover_rate_gte"],
                "macd": filters["macd_gte"],
            }
        ]


def test_cli_read_screen_prints_json_payload() -> None:
    result = runner.invoke(
        app,
        [
            "read",
            "screen",
            "--trade-date",
            "2026-03-30",
            "--pct-chg-gte",
            "5",
            "--turnover-rate-gte",
            "3",
            "--macd-gte",
            "0",
        ],
        obj={
            "read_screen_use_case": ReadScreeningResultsUseCase(
                FakeScreenRepository(),
                indicator_service=None,
            )
        },
    )

    assert result.exit_code == 0
    payload = json.loads(result.stdout)
    assert payload["query"]["trade_date"] == "2026-03-30"
    assert payload["query"]["filters"] == {
        "pct_chg_gte": 5.0,
        "turnover_rate_gte": 3.0,
        "macd_gte": 0.0,
    }
    assert payload["meta"]["matched"] == 1
    assert payload["data"][0]["ts_code"] == "300001.SZ"


class FakeStatsDateRangeUseCase:
    async def run(self) -> dict[str, object]:
        return {
            "data": {
                "daily_market": {
                    "min_trade_date": "2026-01-02",
                    "max_trade_date": "2026-02-10",
                    "continuous_ranges": [["2026-01-02", "2026-01-05"], ["2026-02-10"]],
                },
                "daily_indicators": {
                    "min_trade_date": "2026-01-02",
                    "max_trade_date": "2026-01-05",
                    "continuous_ranges": [["2026-01-02", "2026-01-05"]],
                },
            }
        }


def test_cli_stats_date_range_prints_json_payload() -> None:
    result = runner.invoke(
        app,
        ["stats", "date-range"],
        obj={"stats_date_range_use_case": FakeStatsDateRangeUseCase()},
    )

    assert result.exit_code == 0
    payload = json.loads(result.stdout)
    assert payload["data"]["daily_market"]["continuous_ranges"] == [["2026-01-02", "2026-01-05"], ["2026-02-10"]]


class FakeDeleteByDateUseCase:
    async def run(self, start_date: str, end_date: str) -> dict[str, object]:
        return {
            "query": {"start_date": "2026-01-01", "end_date": "2026-01-31"},
            "data": {"daily_market_deleted": 12, "daily_indicators_deleted": 9},
            "meta": {"total_deleted_rows": 21},
        }


def test_cli_delete_by_date_prints_json_payload() -> None:
    result = runner.invoke(
        app,
        ["delete", "by-date", "--trade-date", "2026-01-31"],
        obj={"delete_by_date_use_case": FakeDeleteByDateUseCase()},
    )

    assert result.exit_code == 0
    payload = json.loads(result.stdout)
    assert payload["meta"]["total_deleted_rows"] == 21
    assert payload["data"]["daily_market_deleted"] == 12


def test_cli_delete_by_date_rejects_mixing_trade_date_and_range() -> None:
    result = runner.invoke(
        app,
        ["delete", "by-date", "--trade-date", "2026-01-31", "--start-date", "2026-01-01", "--end-date", "2026-01-31"],
    )

    assert result.exit_code == 2
    assert "--trade-date cannot be combined with --start-date/--end-date." in result.output


def test_cli_delete_by_date_requires_complete_range() -> None:
    result = runner.invoke(app, ["delete", "by-date", "--start-date", "2026-01-01"])

    assert result.exit_code == 2
    assert "--start-date and --end-date must be provided together." in result.output


class FakePool:
    def __init__(self) -> None:
        self.closed = False

    async def close(self) -> None:
        self.closed = True


def test_run_read_raw_closes_pool_when_building_live_repository(monkeypatch, sample_dsn: str) -> None:
    pool = FakePool()

    class FakeReadRawUseCase:
        def __init__(self, market_repository: object) -> None:
            self.market_repository = market_repository

        async def run(self, ts_code: str, start_date: str, end_date: str) -> dict[str, object]:
            _ = self.market_repository
            return {"query": {"ts_code": ts_code, "start_date": start_date, "end_date": end_date}}

    async def fake_create_pool(dsn: str) -> FakePool:
        assert dsn == sample_dsn
        return pool

    monkeypatch.setenv("POSTGRES_DSN", sample_dsn)
    monkeypatch.setenv("TUSHARE_TOKEN", "token")
    monkeypatch.setattr("cli.create_pool", fake_create_pool)
    monkeypatch.setattr("cli.ReadRawMarketDataUseCase", FakeReadRawUseCase)

    payload = cli_module.asyncio.run(
        cli_module._run_read_raw(
            ts_code="000001.SZ",
            name=None,
            start_date="2026-03-01",
            end_date="2026-03-31",
            injected_use_case=None,
        )
    )

    assert payload["query"]["ts_code"] == "000001.SZ"
    assert pool.closed is True


def test_run_read_raw_resolves_ts_code_from_name_when_building_live_repository(monkeypatch, sample_dsn: str) -> None:
    pool = FakePool()
    calls: dict[str, object] = {}

    class FakeInstrumentRepository:
        def __init__(self, received_pool: object) -> None:
            assert received_pool is pool

        async def find_by_name(self, name: str) -> Instrument:
            calls["name"] = name
            return Instrument(
                ts_code="000001.SZ",
                symbol="000001",
                name=name,
                exchange="SZ",
                list_status="L",
                is_st=False,
            )

    class FakeReadRawUseCase:
        def __init__(self, market_repository: object) -> None:
            self.market_repository = market_repository

        async def run(self, ts_code: str, start_date: str, end_date: str) -> dict[str, object]:
            _ = self.market_repository
            return {"query": {"ts_code": ts_code, "start_date": start_date, "end_date": end_date}}

    async def fake_create_pool(dsn: str) -> FakePool:
        assert dsn == sample_dsn
        return pool

    monkeypatch.setenv("POSTGRES_DSN", sample_dsn)
    monkeypatch.setenv("TUSHARE_TOKEN", "token")
    monkeypatch.setattr("cli.create_pool", fake_create_pool)
    monkeypatch.setattr(
        "cli.MarketDataRepository",
        lambda received_pool, write_batch_size: object(),
    )
    monkeypatch.setattr("cli.InstrumentRepository", FakeInstrumentRepository)
    monkeypatch.setattr("cli.ReadRawMarketDataUseCase", FakeReadRawUseCase)

    payload = cli_module.asyncio.run(
        cli_module._run_read_raw(
            ts_code=None,
            name="Ping An",
            start_date="2026-03-01",
            end_date="2026-03-31",
            injected_use_case=None,
        )
    )

    assert payload["query"]["ts_code"] == "000001.SZ"
    assert calls == {"name": "Ping An"}
    assert pool.closed is True


def test_run_read_screen_closes_pool_when_building_live_repository(monkeypatch, sample_dsn: str) -> None:
    pool = FakePool()

    class FakeReadScreenUseCase:
        def __init__(self, market_repository: object, indicator_service: object | None) -> None:
            self.market_repository = market_repository
            self.indicator_service = indicator_service

        async def run(self, trade_date: str, filters: dict[str, object]) -> dict[str, object]:
            _ = (self.market_repository, self.indicator_service)
            return {"query": {"trade_date": trade_date, "filters": filters}}

    async def fake_create_pool(dsn: str) -> FakePool:
        assert dsn == sample_dsn
        return pool

    monkeypatch.setenv("POSTGRES_DSN", sample_dsn)
    monkeypatch.setenv("TUSHARE_TOKEN", "token")
    monkeypatch.setattr("cli.create_pool", fake_create_pool)
    monkeypatch.setattr("cli.ReadScreeningResultsUseCase", FakeReadScreenUseCase)

    payload = cli_module.asyncio.run(
        cli_module._run_read_screen(
            trade_date="2026-03-31",
            filters={"pct_chg_gte": 5.0},
            injected_use_case=None,
        )
    )

    assert payload["query"]["trade_date"] == "2026-03-31"
    assert pool.closed is True


def test_run_read_raw_passes_configured_batch_size_to_market_repository(monkeypatch, sample_dsn: str) -> None:
    pool = FakePool()
    calls: dict[str, object] = {}

    class FakeMarketDataRepository:
        def __init__(self, received_pool: object, write_batch_size: int) -> None:
            calls["pool"] = received_pool
            calls["write_batch_size"] = write_batch_size

    class FakeReadRawUseCase:
        def __init__(self, market_repository: object) -> None:
            _ = market_repository

        async def run(self, ts_code: str, start_date: str, end_date: str) -> dict[str, object]:
            return {"query": {"ts_code": ts_code, "start_date": start_date, "end_date": end_date}}

    async def fake_create_pool(dsn: str) -> FakePool:
        assert dsn == sample_dsn
        return pool

    monkeypatch.setenv("POSTGRES_DSN", sample_dsn)
    monkeypatch.setenv("TUSHARE_TOKEN", "token")
    monkeypatch.setenv("WRITE_BATCH_SIZE", "321")
    monkeypatch.setattr("cli.create_pool", fake_create_pool)
    monkeypatch.setattr("cli.MarketDataRepository", FakeMarketDataRepository)
    monkeypatch.setattr("cli.ReadRawMarketDataUseCase", FakeReadRawUseCase)

    payload = cli_module.asyncio.run(
        cli_module._run_read_raw(
            ts_code="000001.SZ",
            name=None,
            start_date="2026-03-01",
            end_date="2026-03-31",
            injected_use_case=None,
        )
    )

    assert payload["query"]["ts_code"] == "000001.SZ"
    assert calls == {"pool": pool, "write_batch_size": 321}
    assert pool.closed is True


def test_run_write_resolves_single_name_when_building_live_use_case(monkeypatch, sample_dsn: str) -> None:
    pool = FakePool()
    calls: dict[str, object] = {}

    class FakeInstrumentRepository:
        def __init__(self, received_pool: object) -> None:
            assert received_pool is pool

        async def find_by_name(self, name: str) -> Instrument:
            calls["name"] = name
            return Instrument(
                ts_code="000001.SZ",
                symbol="000001",
                name=name,
                exchange="SZ",
                list_status="L",
                is_st=False,
            )

        async def upsert_instruments(self, instruments: list[Instrument]) -> None:
            _ = instruments

    class FakeWriteMarketDataUseCase:
        def __init__(
            self,
            settings: object,
            primary_provider: object,
            market_repository: object,
            instrument_repository: object,
            job_run_repository: object,
        ) -> None:
            _ = (settings, primary_provider, market_repository, instrument_repository, job_run_repository)

        async def run(
            self,
            mode: str,
            symbols: list[str] | None = None,
            write_range: object | None = None,
            progress: object | None = None,
        ) -> FakeJobRunSummary:
            _ = progress
            calls["mode"] = mode
            calls["symbols"] = symbols
            calls["write_range"] = write_range
            return FakeJobRunSummary(
                job_id="20260331T120000Z",
                status="success",
                started_at="2026-03-31T12:00:00+00:00",
                finished_at="2026-03-31T12:00:01+00:00",
                total_symbols=1,
                success_symbols=["000001.SZ"],
                failed_symbols={},
            )

    async def fake_create_pool(dsn: str) -> FakePool:
        assert dsn == sample_dsn
        return pool

    monkeypatch.setenv("POSTGRES_DSN", sample_dsn)
    monkeypatch.setenv("TUSHARE_TOKEN", "token")
    monkeypatch.setattr("cli.create_pool", fake_create_pool)
    monkeypatch.setattr(
        "cli.MarketDataRepository",
        lambda received_pool, write_batch_size: object(),
    )
    monkeypatch.setattr("cli.InstrumentRepository", FakeInstrumentRepository)
    monkeypatch.setattr("cli.JobRunRepository", lambda received_pool: object())
    monkeypatch.setattr("cli.WriteMarketDataUseCase", FakeWriteMarketDataUseCase)

    payload = cli_module.asyncio.run(
        cli_module._run_write(
            mode="single",
            ts_code=None,
            name="Ping An",
            write_range=None,
            injected_use_case=None,
            progress=None,
        )
    )

    assert payload.success_symbols == ["000001.SZ"]
    assert calls == {
        "name": "Ping An",
        "mode": "single",
        "symbols": ["000001.SZ"],
        "write_range": None,
    }
    assert pool.closed is True


def test_run_write_passes_configured_batch_size_to_market_repository(monkeypatch, sample_dsn: str) -> None:
    pool = FakePool()
    calls: dict[str, object] = {}

    class FakeMarketDataRepository:
        def __init__(self, received_pool: object, write_batch_size: int) -> None:
            calls["pool"] = received_pool
            calls["write_batch_size"] = write_batch_size

    class FakeWriteMarketDataUseCase:
        def __init__(
            self,
            settings: object,
            primary_provider: object,
            market_repository: object,
            instrument_repository: object,
            job_run_repository: object,
        ) -> None:
            _ = (settings, primary_provider, market_repository, instrument_repository, job_run_repository)

        async def run(
            self,
            mode: str,
            symbols: list[str] | None = None,
            write_range: object | None = None,
            progress: object | None = None,
        ) -> FakeJobRunSummary:
            _ = (mode, symbols, write_range, progress)
            return FakeJobRunSummary(
                job_id="20260331T120000Z",
                status="success",
                started_at="2026-03-31T12:00:00+00:00",
                finished_at="2026-03-31T12:00:01+00:00",
                total_symbols=1,
                success_symbols=["000001.SZ"],
                failed_symbols={},
            )

    async def fake_create_pool(dsn: str) -> FakePool:
        assert dsn == sample_dsn
        return pool

    monkeypatch.setenv("POSTGRES_DSN", sample_dsn)
    monkeypatch.setenv("TUSHARE_TOKEN", "token")
    monkeypatch.setenv("WRITE_BATCH_SIZE", "777")
    monkeypatch.setattr("cli.create_pool", fake_create_pool)
    monkeypatch.setattr("cli.MarketDataRepository", FakeMarketDataRepository)
    monkeypatch.setattr("cli.InstrumentRepository", lambda received_pool: object())
    monkeypatch.setattr("cli.JobRunRepository", lambda received_pool: object())
    monkeypatch.setattr("cli.WriteMarketDataUseCase", FakeWriteMarketDataUseCase)

    payload = cli_module.asyncio.run(
        cli_module._run_write(
            mode="full",
            ts_code=None,
            name=None,
            write_range=None,
            injected_use_case=None,
            progress=None,
        )
    )

    assert payload.status == "success"
    assert calls == {"pool": pool, "write_batch_size": 777}
    assert pool.closed is True


@dataclass
class FakeJobRunSummary:
    job_id: str
    status: str
    started_at: str
    finished_at: str
    total_symbols: int
    success_symbols: list[str]
    failed_symbols: dict[str, str]


class FakeWriteUseCase:
    def __init__(self) -> None:
        self.calls: list[tuple[str, list[str] | None, object | None]] = []

    async def run(
        self,
        mode: str,
        symbols: list[str] | None = None,
        write_range: object | None = None,
        progress: object | None = None,
    ) -> FakeJobRunSummary:
        _ = progress
        self.calls.append((mode, symbols, write_range))
        return FakeJobRunSummary(
            job_id="20260331T120000Z",
            status="success",
            started_at="2026-03-31T12:00:00+00:00",
            finished_at="2026-03-31T12:00:01+00:00",
            total_symbols=1,
            success_symbols=["000001.SZ"],
            failed_symbols={},
        )


class FakePartialWriteUseCase:
    async def run(
        self,
        mode: str,
        symbols: list[str] | None = None,
        write_range: object | None = None,
        progress: object | None = None,
    ) -> FakeJobRunSummary:
        _ = (mode, symbols, write_range, progress)
        return FakeJobRunSummary(
            job_id="20260331T120000Z",
            status="partial_success",
            started_at="2026-03-31T12:00:00+00:00",
            finished_at="2026-03-31T12:00:02+00:00",
            total_symbols=3,
            success_symbols=["000001.SZ", "000002.SZ"],
            failed_symbols={"000003.SZ": "upstream timeout"},
        )


def test_cli_write_runs_use_case_with_mode_option() -> None:
    use_case = FakeWriteUseCase()

    result = runner.invoke(
        app,
        ["write", "--mode", "full"],
        obj={"write_use_case": use_case},
    )

    assert result.exit_code == 0
    assert use_case.calls == [("full", None, None)]
    payload = json.loads(result.stdout)
    assert payload == {
        "job_id": "20260331T120000Z",
        "status": "success",
        "started_at": "2026-03-31T12:00:00+00:00",
        "finished_at": "2026-03-31T12:00:01+00:00",
        "total_symbols": 1,
        "success_count": 1,
        "failed_count": 0,
    }


def test_cli_write_rejects_removed_failed_only_mode() -> None:
    result = runner.invoke(app, ["write", "--mode", "failed-only"])

    assert result.exit_code == 2
    assert "failed-only" in result.output


def test_cli_write_single_requires_exactly_one_lookup_selector() -> None:
    result = runner.invoke(app, ["write", "--mode", "single"])

    assert result.exit_code == 2
    assert "Exactly one of --ts-code or --name must be provided" in result.output
    assert "--mode single" in result.output


def test_cli_write_full_rejects_lookup_selectors() -> None:
    result = runner.invoke(app, ["write", "--mode", "full", "--ts-code", "000001.SZ"])

    assert result.exit_code == 2
    assert "--ts-code/--name can only be used with" in result.output
    assert "--mode single" in result.output


def test_cli_write_single_passes_ts_code_selector() -> None:
    use_case = FakeWriteUseCase()

    result = runner.invoke(
        app,
        ["write", "--mode", "single", "--ts-code", "000001.SZ"],
        obj={"write_use_case": use_case},
    )

    assert result.exit_code == 0
    assert use_case.calls == [("single", ["000001.SZ"], None)]
    payload = json.loads(result.stdout)
    assert payload["status"] == "success"


def test_cli_write_outputs_summary_counts_without_symbol_lists() -> None:
    result = runner.invoke(
        app,
        ["write", "--mode", "full"],
        obj={"write_use_case": FakePartialWriteUseCase()},
    )

    assert result.exit_code == 0
    payload = json.loads(result.stdout)
    assert payload == {
        "job_id": "20260331T120000Z",
        "status": "partial_success",
        "started_at": "2026-03-31T12:00:00+00:00",
        "finished_at": "2026-03-31T12:00:02+00:00",
        "total_symbols": 3,
        "success_count": 2,
        "failed_count": 1,
    }


def test_cli_write_passes_lookback_override(monkeypatch) -> None:
    captured: dict[str, object] = {}

    async def fake_run_write(
        mode: str,
        ts_code: str | None,
        name: str | None,
        write_range: object | None,
        injected_use_case: object | None,
        progress: object | None,
    ) -> object:
        captured["mode"] = mode
        captured["ts_code"] = ts_code
        captured["name"] = name
        captured["write_range"] = write_range
        captured["injected_use_case"] = injected_use_case
        captured["progress"] = progress
        return FakeJobRunSummary(
            job_id="20260331T120000Z",
            status="success",
            started_at="2026-03-31T12:00:00+00:00",
            finished_at="2026-03-31T12:00:01+00:00",
            total_symbols=1,
            success_symbols=["000001.SZ"],
            failed_symbols={},
        )

    monkeypatch.setattr("cli._run_write", fake_run_write, raising=False)

    result = runner.invoke(app, ["write", "--mode", "full", "--lookback-trading-days", "30"])

    assert result.exit_code == 0
    assert captured["mode"] == "full"
    assert captured["ts_code"] is None
    assert captured["name"] is None
    assert captured["injected_use_case"] is None
    assert callable(captured["progress"])
    assert captured["write_range"].lookback_trading_days == 30
    assert captured["write_range"].start_date is None
    assert captured["write_range"].end_date is None


def test_cli_write_passes_absolute_date_range(monkeypatch) -> None:
    captured: dict[str, object] = {}

    async def fake_run_write(
        mode: str,
        ts_code: str | None,
        name: str | None,
        write_range: object | None,
        injected_use_case: object | None,
        progress: object | None,
    ) -> object:
        captured["mode"] = mode
        captured["ts_code"] = ts_code
        captured["name"] = name
        captured["write_range"] = write_range
        captured["progress"] = progress
        return FakeJobRunSummary(
            job_id="20260331T120000Z",
            status="success",
            started_at="2026-03-31T12:00:00+00:00",
            finished_at="2026-03-31T12:00:01+00:00",
            total_symbols=1,
            success_symbols=["000001.SZ"],
            failed_symbols={},
        )

    monkeypatch.setattr("cli._run_write", fake_run_write, raising=False)

    result = runner.invoke(
        app,
        [
            "write",
            "--mode",
            "full",
            "--start-date",
            "2026-01-01",
            "--end-date",
            "2026-03-31",
        ],
    )

    assert result.exit_code == 0
    assert captured["mode"] == "full"
    assert captured["ts_code"] is None
    assert captured["name"] is None
    assert callable(captured["progress"])
    assert captured["write_range"].lookback_trading_days is None
    assert captured["write_range"].start_date == "20260101"
    assert captured["write_range"].end_date == "20260331"


def test_cli_write_rejects_mixing_lookback_and_date_range() -> None:
    result = runner.invoke(
        app,
        [
            "write",
            "--mode",
            "full",
            "--lookback-trading-days",
            "30",
            "--start-date",
            "2026-01-01",
            "--end-date",
            "2026-03-31",
        ],
    )

    assert result.exit_code == 2
    assert "--lookback-trading-days" in result.output
    assert "cannot be combined" in result.output


def test_cli_write_requires_complete_date_range() -> None:
    result = runner.invoke(app, ["write", "--mode", "full", "--start-date", "2026-01-01"])

    assert result.exit_code == 2
    assert "--start-date and --end-date must be provided together." in result.output


def test_cli_write_rejects_reversed_date_range() -> None:
    result = runner.invoke(
        app,
        ["write", "--mode", "full", "--start-date", "2026-03-31", "--end-date", "2026-01-01"],
    )

    assert result.exit_code == 2
    assert "--start-date must be earlier than or equal to --end-date." in result.output


def test_cli_write_reports_postgres_precheck_failure(monkeypatch) -> None:
    async def fake_run_write(
        mode: str,
        ts_code: str | None,
        name: str | None,
        write_range: object | None,
        injected_use_case: object | None,
        progress: object | None,
    ) -> object:
        _ = (mode, ts_code, name, write_range, injected_use_case, progress)
        raise DatabasePrecheckError("PostgreSQL is not reachable at configured POSTGRES_DSN.")

    monkeypatch.setattr("cli._run_write", fake_run_write, raising=False)

    result = runner.invoke(app, ["write", "--mode", "full"])

    assert result.exit_code == 1
    payload = json.loads(result.stdout)
    assert payload == {
        "status": "error",
        "error": "postgres_unreachable",
        "message": "PostgreSQL is not reachable at configured POSTGRES_DSN.",
    }


def test_cli_write_emits_progress_to_stderr(monkeypatch) -> None:
    async def fake_run_write(
        mode: str,
        ts_code: str | None,
        name: str | None,
        write_range: object | None,
        injected_use_case: object | None,
        progress: object | None,
    ) -> object:
        _ = (mode, ts_code, name, write_range, injected_use_case)
        assert callable(progress)
        progress("starting write")
        progress("finished write")
        return FakeJobRunSummary(
            job_id="20260331T120000Z",
            status="success",
            started_at="2026-03-31T12:00:00+00:00",
            finished_at="2026-03-31T12:00:01+00:00",
            total_symbols=1,
            success_symbols=["000001.SZ"],
            failed_symbols={},
        )

    monkeypatch.setattr("cli._run_write", fake_run_write, raising=False)

    result = runner.invoke(app, ["write", "--mode", "full"])

    assert result.exit_code == 0
    assert result.stderr == "starting write\nfinished write\n"
    payload = json.loads(result.stdout)
    assert payload["status"] == "success"


def test_cli_write_indexes_mode_passes_absolute_date_range(monkeypatch) -> None:
    captured: dict[str, object] = {}

    async def fake_run_write(
        mode: str,
        ts_code: str | None,
        name: str | None,
        write_range: object | None,
        injected_use_case: object | None,
        progress: object | None,
    ) -> object:
        captured["mode"] = mode
        captured["ts_code"] = ts_code
        captured["name"] = name
        captured["write_range"] = write_range
        captured["injected_use_case"] = injected_use_case
        captured["progress"] = progress
        return FakeJobRunSummary(
            job_id="20260331T120000Z",
            status="success",
            started_at="2026-03-31T12:00:00+00:00",
            finished_at="2026-03-31T12:00:01+00:00",
            total_symbols=0,
            success_symbols=[],
            failed_symbols={},
        )

    monkeypatch.setattr("cli._run_write", fake_run_write, raising=False)

    result = runner.invoke(
        app,
        ["write", "--mode", "indexes", "--start-date", "2026-01-01", "--end-date", "2026-03-31"],
    )

    assert result.exit_code == 0
    assert captured["mode"] == "indexes"
    assert captured["ts_code"] is None
    assert captured["name"] is None
    assert captured["write_range"].start_date == "20260101"
    assert captured["write_range"].end_date == "20260331"
    assert callable(captured["progress"])


def test_cli_write_indexes_mode_rejects_mixing_lookback_and_date_range() -> None:
    result = runner.invoke(
        app,
        [
            "write",
            "--mode",
            "indexes",
            "--lookback-trading-days",
            "30",
            "--start-date",
            "2026-01-01",
            "--end-date",
            "2026-03-31",
        ],
    )

    assert result.exit_code == 2
    assert "--lookback-trading-days" in result.output
    assert "cannot be combined" in result.output


def test_cli_write_does_not_instantiate_akshare_adapter(monkeypatch, sample_dsn: str, tmp_path) -> None:
    monkeypatch.setenv("POSTGRES_DSN", sample_dsn)
    monkeypatch.setenv("TUSHARE_TOKEN", "token")
    monkeypatch.setenv("STATUS_FILE_PATH", str(tmp_path / "status.txt"))
    monkeypatch.setenv("DEFAULT_LOOKBACK_TRADING_DAYS", "1")

    class FakeMarketRepository:
        async def upsert_daily_market(self, rows: list[object]) -> None:
            _ = rows

        async def upsert_daily_indicators(self, rows: list[object]) -> None:
            _ = rows

    class FakeInstrumentRepository:
        async def upsert_instruments(self, instruments: list[Instrument]) -> None:
            _ = instruments

    class FakeJobRunRepository:
        async def insert_job_run(self, summary: object, status_file_path: str, job_type: str = "write") -> None:
            _ = (summary, status_file_path, job_type)

    async def fake_create_pool(dsn: str) -> FakePool:
        assert dsn == sample_dsn
        return FakePool()

    def fail_akshare_init() -> None:
        raise AssertionError("AkshareAdapter should not be instantiated")

    def fake_fetch_instruments(self: object) -> list[Instrument]:
        _ = self
        return [
            Instrument(
                ts_code="000001.SZ",
                symbol="000001",
                name="Ping An",
                exchange="SZ",
                list_status="L",
                is_st=False,
            )
        ]

    def fake_fetch_recent_trade_dates(self: object, end_date: str, limit: int) -> list[str]:
        _ = (self, end_date, limit)
        return ["20260331"]

    async def fake_fetch_daily_by_trade_date(self: object, trade_date: str) -> list[dict[str, object]]:
        _ = (self, trade_date)
        return [{"ts_code": "000001.SZ", "trade_date": "20260331", "close": 11.6, "pct_chg": 2.2}]

    async def fake_fetch_daily_basic_by_trade_date(self: object, trade_date: str) -> list[dict[str, object]]:
        _ = (self, trade_date)
        return [{"ts_code": "000001.SZ", "trade_date": "20260331", "turnover_rate": 3.3, "total_mv": 456.7}]

    async def fake_fetch_moneyflow_by_trade_date(self: object, trade_date: str) -> list[dict[str, object]]:
        _ = (self, trade_date)
        return [{"ts_code": "000001.SZ", "trade_date": "20260331", "net_mf_amount": 9.8}]

    async def fake_fetch_adj_factor_by_trade_date(self: object, trade_date: str) -> list[dict[str, object]]:
        _ = (self, trade_date)
        return []

    async def fake_fetch_stk_limit_by_trade_date(self: object, trade_date: str) -> list[dict[str, object]]:
        _ = (self, trade_date)
        return []

    async def fake_fetch_suspend_d_by_trade_date(self: object, trade_date: str) -> list[dict[str, object]]:
        _ = (self, trade_date)
        return []

    async def fake_fetch_indicators_by_trade_date(self: object, trade_date: str) -> list[dict[str, object]]:
        _ = (self, trade_date)
        return [{"ts_code": "000001.SZ", "trade_date": "20260331", "macd": 0.11, "kdj_j": 81.0}]

    def fake_fetch_index_daily(self: object, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        _ = (self, ts_code, start_date, end_date)
        return []

    def fake_fetch_sw_daily(self: object, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        _ = (self, ts_code, start_date, end_date)
        return []

    monkeypatch.setattr(cli_module, "AkshareAdapter", fail_akshare_init, raising=False)
    monkeypatch.setattr("cli.create_pool", fake_create_pool)
    monkeypatch.setattr(
        "cli.MarketDataRepository",
        lambda pool, write_batch_size: FakeMarketRepository(),
    )
    monkeypatch.setattr("cli.InstrumentRepository", lambda pool: FakeInstrumentRepository())
    monkeypatch.setattr("cli.JobRunRepository", lambda pool: FakeJobRunRepository())
    monkeypatch.setattr("providers.tushare_adapter.ts.pro_api", lambda token: object())
    monkeypatch.setattr("providers.tushare_adapter.TushareAdapter.fetch_instruments", fake_fetch_instruments)
    monkeypatch.setattr(
        "providers.tushare_adapter.TushareAdapter.fetch_recent_trade_dates",
        fake_fetch_recent_trade_dates,
    )
    monkeypatch.setattr(
        "providers.tushare_adapter.TushareAdapter.fetch_daily_by_trade_date",
        fake_fetch_daily_by_trade_date,
    )
    monkeypatch.setattr(
        "providers.tushare_adapter.TushareAdapter.fetch_daily_basic_by_trade_date",
        fake_fetch_daily_basic_by_trade_date,
    )
    monkeypatch.setattr(
        "providers.tushare_adapter.TushareAdapter.fetch_moneyflow_by_trade_date",
        fake_fetch_moneyflow_by_trade_date,
    )
    monkeypatch.setattr(
        "providers.tushare_adapter.TushareAdapter.fetch_adj_factor_by_trade_date",
        fake_fetch_adj_factor_by_trade_date,
    )
    monkeypatch.setattr(
        "providers.tushare_adapter.TushareAdapter.fetch_stk_limit_by_trade_date",
        fake_fetch_stk_limit_by_trade_date,
    )
    monkeypatch.setattr(
        "providers.tushare_adapter.TushareAdapter.fetch_suspend_d_by_trade_date",
        fake_fetch_suspend_d_by_trade_date,
    )
    monkeypatch.setattr(
        "providers.tushare_adapter.TushareAdapter.fetch_indicators_by_trade_date",
        fake_fetch_indicators_by_trade_date,
    )
    monkeypatch.setattr("providers.tushare_adapter.TushareAdapter.fetch_index_daily", fake_fetch_index_daily)
    monkeypatch.setattr("providers.tushare_adapter.TushareAdapter.fetch_sw_daily", fake_fetch_sw_daily)

    result = runner.invoke(app, ["write", "--mode", "full"])

    assert result.exit_code == 0
    payload = json.loads(result.stdout)
    assert payload["status"] == "success"


def test_cli_init_db_prints_json_payload(monkeypatch) -> None:
    async def fake_run_init_db(injected_result: object | None = None) -> dict[str, object]:
        _ = injected_result
        return {
            "status": "ok",
            "created_tables": ["daily_market"],
            "already_present": ["instruments", "daily_indicators", "job_runs"],
            "missing": [],
        }

    monkeypatch.setattr("cli._run_init_db", fake_run_init_db, raising=False)

    result = runner.invoke(app, ["init-db"])

    assert result.exit_code == 0
    payload = json.loads(result.stdout)
    assert payload == {
        "status": "ok",
        "created_tables": ["daily_market"],
        "already_present": ["instruments", "daily_indicators", "job_runs"],
        "missing": [],
    }


class FakeFrame:
    def __init__(self, records: list[dict[str, object]]) -> None:
        self.records = records
        self.orient: str | None = None

    def to_dict(self, orient: str) -> list[dict[str, object]]:
        self.orient = orient
        return self.records


class FakeTushareProClient:
    def __init__(self, frame: FakeFrame) -> None:
        self.frame = frame
        self.calls: list[dict[str, str]] = []

    def daily(self, **kwargs: str) -> FakeFrame:
        self.calls.append({"endpoint": "daily", **kwargs})
        return self.frame

    def stock_basic(self, **kwargs: str) -> FakeFrame:
        self.calls.append({"endpoint": "stock_basic", **kwargs})
        return self.frame

    def trade_cal(self, **kwargs: str) -> FakeFrame:
        self.calls.append({"endpoint": "trade_cal", **kwargs})
        return self.frame

    def daily_basic(self, **kwargs: str) -> FakeFrame:
        self.calls.append({"endpoint": "daily_basic", **kwargs})
        return self.frame

    def moneyflow(self, **kwargs: str) -> FakeFrame:
        self.calls.append({"endpoint": "moneyflow", **kwargs})
        return self.frame

    def stk_factor(self, **kwargs: str) -> FakeFrame:
        self.calls.append({"endpoint": "stk_factor", **kwargs})
        return self.frame


def test_tushare_adapter_fetch_daily_converts_dataframe_to_records(monkeypatch) -> None:
    captured: dict[str, object] = {}
    frame = FakeFrame([{"ts_code": "000001.SZ", "trade_date": "20260330", "close": 12.4}])
    client = FakeTushareProClient(frame)

    def fake_pro_api(token: str) -> FakeTushareProClient:
        captured["token"] = token
        return client

    monkeypatch.setattr("providers.tushare_adapter.ts.pro_api", fake_pro_api)

    adapter = TushareAdapter("demo-token")
    rows = adapter.fetch_daily("000001.SZ", "20260101", "20260330")

    assert captured["token"] == "demo-token"
    assert client.calls == [
        {"endpoint": "daily", "ts_code": "000001.SZ", "start_date": "20260101", "end_date": "20260330"}
    ]
    assert frame.orient == "records"
    assert rows == [{"ts_code": "000001.SZ", "trade_date": "20260330", "close": 12.4, "source_daily": "daily"}]


def test_tushare_adapter_fetch_instruments_maps_stock_basic_rows(monkeypatch) -> None:
    frame = FakeFrame(
        [
            {
                "ts_code": "000001.SZ",
                "symbol": "000001",
                "name": "Ping An Bank",
                "exchange": "SZSE",
                "list_status": "L",
            }
        ]
    )
    client = FakeTushareProClient(frame)

    monkeypatch.setattr("providers.tushare_adapter.ts.pro_api", lambda token: client)

    adapter = TushareAdapter("demo-token")
    instruments = list(adapter.fetch_instruments())

    assert client.calls == [{"endpoint": "stock_basic", "list_status": "L"}]
    assert len(instruments) == 1
    assert instruments[0].ts_code == "000001.SZ"
    assert instruments[0].symbol == "000001"
    assert instruments[0].name == "Ping An Bank"
    assert instruments[0].exchange == "SZSE"
    assert instruments[0].list_status == "L"
    assert instruments[0].is_st is False


def test_tushare_adapter_fetch_instruments_infers_exchange_from_ts_code_when_missing(monkeypatch) -> None:
    frame = FakeFrame(
        [
            {
                "ts_code": "600000.SH",
                "symbol": "600000",
                "name": "浦发银行",
                "list_status": "L",
            }
        ]
    )
    client = FakeTushareProClient(frame)

    monkeypatch.setattr("providers.tushare_adapter.ts.pro_api", lambda token: client)

    adapter = TushareAdapter("demo-token")
    instruments = list(adapter.fetch_instruments())

    assert len(instruments) == 1
    assert instruments[0].exchange == "SSE"
    assert instruments[0].list_status == "L"


def test_tushare_adapter_fetch_recent_trade_dates_returns_open_days(monkeypatch) -> None:
    frame = FakeFrame(
        [
            {"cal_date": "20260331", "is_open": 1},
            {"cal_date": "20260330", "is_open": 1},
            {"cal_date": "20260329", "is_open": 0},
        ]
    )
    client = FakeTushareProClient(frame)

    monkeypatch.setattr("providers.tushare_adapter.ts.pro_api", lambda token: client)

    adapter = TushareAdapter("demo-token")
    dates = list(adapter.fetch_recent_trade_dates("20260331", 2))

    assert client.calls == [
        {
            "endpoint": "trade_cal",
            "exchange": "SSE",
            "start_date": "20260324",
            "end_date": "20260331",
        }
    ]
    assert dates == ["20260331", "20260330"]


def test_tushare_adapter_fetch_trade_dates_in_range_returns_open_days(monkeypatch) -> None:
    frame = FakeFrame(
        [
            {"cal_date": "20260101", "is_open": 0},
            {"cal_date": "20260102", "is_open": 1},
            {"cal_date": "20260103", "is_open": 0},
            {"cal_date": "20260105", "is_open": 1},
        ]
    )
    client = FakeTushareProClient(frame)

    monkeypatch.setattr("providers.tushare_adapter.ts.pro_api", lambda token: client)

    adapter = TushareAdapter("demo-token")
    dates = list(adapter.fetch_trade_dates_in_range("20260101", "20260105"))

    assert client.calls == [
        {
            "endpoint": "trade_cal",
            "exchange": "SSE",
            "start_date": "20260101",
            "end_date": "20260105",
        }
    ]
    assert dates == ["20260102", "20260105"]


def test_tushare_adapter_fetch_daily_basic_converts_dataframe_to_records(monkeypatch) -> None:
    frame = FakeFrame([{"trade_date": "20260331", "turnover_rate": 3.3, "total_mv": 456.7}])
    client = FakeTushareProClient(frame)

    monkeypatch.setattr("providers.tushare_adapter.ts.pro_api", lambda token: client)

    adapter = TushareAdapter("demo-token")
    rows = adapter.fetch_daily_basic("000001.SZ", "20260324", "20260331")

    assert client.calls == [
        {
            "endpoint": "daily_basic",
            "ts_code": "000001.SZ",
            "start_date": "20260324",
            "end_date": "20260331",
        }
    ]
    assert rows == [
        {"trade_date": "20260331", "turnover_rate": 3.3, "total_mv": 456.7, "source_daily_basic": "daily_basic"}
    ]


def test_tushare_adapter_fetch_moneyflow_converts_dataframe_to_records(monkeypatch) -> None:
    frame = FakeFrame([{"trade_date": "20260331", "net_mf_amount": 9.8}])
    client = FakeTushareProClient(frame)

    monkeypatch.setattr("providers.tushare_adapter.ts.pro_api", lambda token: client)

    adapter = TushareAdapter("demo-token")
    rows = adapter.fetch_moneyflow("000001.SZ", "20260324", "20260331")

    assert client.calls == [
        {
            "endpoint": "moneyflow",
            "ts_code": "000001.SZ",
            "start_date": "20260324",
            "end_date": "20260331",
        }
    ]
    assert rows == [{"trade_date": "20260331", "net_mf_amount": 9.8, "source_moneyflow": "moneyflow"}]


def test_tushare_adapter_fetch_indicators_converts_dataframe_to_records(monkeypatch) -> None:
    frame = FakeFrame([{"trade_date": "20260331", "macd": 0.11, "kdj_j": 81.0}])
    client = FakeTushareProClient(frame)

    monkeypatch.setattr("providers.tushare_adapter.ts.pro_api", lambda token: client)

    adapter = TushareAdapter("demo-token")
    rows = adapter.fetch_indicators("000001.SZ", "20260324", "20260331")

    assert client.calls == [
        {
            "endpoint": "stk_factor",
            "ts_code": "000001.SZ",
            "start_date": "20260324",
            "end_date": "20260331",
        }
    ]
    assert rows == [{"trade_date": "20260331", "macd": 0.11, "kdj_j": 81.0}]


@pytest.mark.asyncio
async def test_tushare_adapter_fetch_daily_by_trade_date_converts_dataframe_to_records(monkeypatch) -> None:
    frame = FakeFrame([{"ts_code": "000001.SZ", "trade_date": "20260331", "close": 12.4}])
    client = FakeTushareProClient(frame)

    monkeypatch.setattr("providers.tushare_adapter.ts.pro_api", lambda token: client)

    adapter = TushareAdapter("demo-token")
    rows = await adapter.fetch_daily_by_trade_date("20260331")

    assert client.calls == [
        {"endpoint": "daily", "trade_date": "20260331"}
    ]
    assert rows == [{"ts_code": "000001.SZ", "trade_date": "20260331", "close": 12.4, "source_daily": "daily"}]


@pytest.mark.asyncio
async def test_tushare_adapter_fetch_indicators_by_trade_date_falls_back_to_stk_factor(monkeypatch) -> None:
    fallback_frame = FakeFrame([{"ts_code": "000001.SZ", "trade_date": "20260331", "macd": 0.11}])

    class FactorFallbackClient:
        def __init__(self) -> None:
            self.calls: list[dict[str, str]] = []

        def stk_factor_pro(self, **kwargs: str) -> FakeFrame:
            self.calls.append({"endpoint": "stk_factor_pro", **kwargs})
            raise Exception("抱歉，您没有访问该接口的权限")

        def stk_factor(self, **kwargs: str) -> FakeFrame:
            self.calls.append({"endpoint": "stk_factor", **kwargs})
            return fallback_frame

    client = FactorFallbackClient()
    monkeypatch.setattr("providers.tushare_adapter.ts.pro_api", lambda token: client)

    adapter = TushareAdapter("demo-token")
    rows = await adapter.fetch_indicators_by_trade_date("20260331")

    assert client.calls == [
        {"endpoint": "stk_factor_pro", "trade_date": "20260331"},
        {"endpoint": "stk_factor", "trade_date": "20260331"},
    ]
    assert rows == [
        {
            "ts_code": "000001.SZ",
            "trade_date": "20260331",
            "macd": 0.11,
            "source_interface": "stk_factor",
        }
    ]


def test_tushare_adapter_wraps_network_errors_as_retryable_provider_error(monkeypatch) -> None:
    class FailingClient:
        def daily(self, **kwargs: str) -> FakeFrame:
            raise RequestsConnectionError("dns failed")

    monkeypatch.setattr("providers.tushare_adapter.ts.pro_api", lambda token: FailingClient())

    adapter = TushareAdapter("demo-token")

    from domain.errors import RetryableProviderError

    with pytest.raises(RetryableProviderError, match="dns failed"):
        adapter.fetch_daily("000001.SZ", "20260324", "20260331")


def test_tushare_adapter_wraps_slow_calls_as_retryable_provider_error(monkeypatch) -> None:
    class SlowClient:
        def daily(self, **kwargs: str) -> FakeFrame:
            time.sleep(0.05)
            return FakeFrame([])

    monkeypatch.setattr("providers.tushare_adapter.ts.pro_api", lambda token: SlowClient())

    adapter = TushareAdapter("demo-token", timeout_seconds=0.01)

    from domain.errors import RetryableProviderError

    started = time.perf_counter()
    with pytest.raises(RetryableProviderError, match="timed out"):
        adapter.fetch_daily("000001.SZ", "20260324", "20260331")
    assert time.perf_counter() - started < 0.03
