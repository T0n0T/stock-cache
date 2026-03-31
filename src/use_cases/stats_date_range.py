from datetime import datetime


def _compact_date(value: str) -> str:
    return datetime.strptime(value, "%Y-%m-%d").strftime("%Y%m%d")


def _iso_date(value: str) -> str:
    if "-" in value:
        return value
    return datetime.strptime(value, "%Y%m%d").date().isoformat()


def _build_continuous_ranges(stored_dates: list[str], expected_dates: list[str]) -> list[list[str]]:
    if not stored_dates:
        return []

    stored_set = set(stored_dates)
    normalized_expected = [_iso_date(value) for value in expected_dates]
    ranges: list[list[str]] = []
    current: list[str] = []

    for trade_date in normalized_expected:
        if trade_date in stored_set:
            current.append(trade_date)
        elif current:
            ranges.append(current)
            current = []

    if current:
        ranges.append(current)

    if ranges:
        return ranges
    return [stored_dates]


class StatsDateRangeUseCase:
    def __init__(self, market_repository: object, trade_calendar_provider: object) -> None:
        self._market_repository = market_repository
        self._trade_calendar_provider = trade_calendar_provider

    async def run(self) -> dict[str, object]:
        inventory = await self._market_repository.fetch_trade_date_inventory()
        return {
            "data": {
                table_name: self._build_section(trade_dates)
                for table_name, trade_dates in inventory.items()
            }
        }

    def _build_section(self, trade_dates: list[str]) -> dict[str, object]:
        if not trade_dates:
            return {
                "min_trade_date": None,
                "max_trade_date": None,
                "continuous_ranges": [],
            }

        min_trade_date = trade_dates[0]
        max_trade_date = trade_dates[-1]
        expected_dates = self._trade_calendar_provider.fetch_trade_dates_in_range(
            _compact_date(min_trade_date),
            _compact_date(max_trade_date),
        )
        return {
            "min_trade_date": min_trade_date,
            "max_trade_date": max_trade_date,
            "continuous_ranges": _build_continuous_ranges(trade_dates, list(expected_dates)),
        }
