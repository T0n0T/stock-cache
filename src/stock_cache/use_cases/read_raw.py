from dataclasses import asdict


class ReadRawMarketDataUseCase:
    def __init__(self, market_repository: object) -> None:
        self._market_repository = market_repository

    async def run(self, ts_code: str, start_date: str, end_date: str) -> dict[str, object]:
        rows = await self._market_repository.fetch_raw(ts_code=ts_code, start_date=start_date, end_date=end_date)
        market = [asdict(row) for row in rows["market"]]
        indicators = [asdict(row) for row in rows["indicators"]]
        return {
            "query": {"ts_code": ts_code, "start_date": start_date, "end_date": end_date},
            "data": {"market": market, "indicators": indicators},
            "meta": {
                "row_count_market": len(market),
                "row_count_indicators": len(indicators),
            },
        }
