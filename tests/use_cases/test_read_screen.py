import pytest

from stock_cache.use_cases.read_screen import ReadScreeningResultsUseCase


class FakeScreenRepository:
    async def screen(self, trade_date: str, filters: dict[str, object]) -> list[dict[str, object]]:
        return [
            {
                "ts_code": "300001.SZ",
                "name": "Tech Corp",
                "trade_date": trade_date,
                "pct_chg": 6.2,
                "turnover_rate": 4.8,
                "total_mv": 28000000000,
                "macd": 0.13,
                "kdj_j": 91.4,
            }
        ]


@pytest.mark.asyncio
async def test_screen_read_returns_matches_and_meta() -> None:
    use_case = ReadScreeningResultsUseCase(FakeScreenRepository(), indicator_service=None)

    payload = await use_case.run(
        trade_date="2026-03-30",
        filters={"pct_chg_gte": 5, "turnover_rate_gte": 3, "macd_gte": 0},
    )

    assert payload["meta"]["matched"] == 1
    assert payload["data"][0]["ts_code"] == "300001.SZ"
