class ReadScreeningResultsUseCase:
    def __init__(self, market_repository: object, indicator_service: object | None) -> None:
        self._market_repository = market_repository
        self._indicator_service = indicator_service

    async def run(self, trade_date: str, filters: dict[str, object]) -> dict[str, object]:
        rows = await self._market_repository.screen(trade_date=trade_date, filters=filters)
        return {
            "query": {"trade_date": trade_date, "filters": filters},
            "data": rows,
            "meta": {"matched": len(rows)},
        }
