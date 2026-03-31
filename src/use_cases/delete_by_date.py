from datetime import datetime


def _iso_date(value: str) -> str:
    return datetime.strptime(value, "%Y%m%d").date().isoformat()


class DeleteByDateUseCase:
    def __init__(self, market_repository: object) -> None:
        self._market_repository = market_repository

    async def run(self, start_date: str, end_date: str) -> dict[str, object]:
        deleted = await self._market_repository.delete_trade_date_range(start_date=start_date, end_date=end_date)
        return {
            "query": {"start_date": _iso_date(start_date), "end_date": _iso_date(end_date)},
            "data": deleted,
            "meta": {"total_deleted_rows": sum(deleted.values())},
        }
