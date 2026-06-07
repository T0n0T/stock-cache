from dataclasses import asdict
from datetime import date, datetime


def _json_ready(value: object) -> object:
    if isinstance(value, (date, datetime)):
        return value.isoformat()
    if isinstance(value, dict):
        return {key: _json_ready(item) for key, item in value.items()}
    if isinstance(value, list):
        return [_json_ready(item) for item in value]
    return value


class ReadRawMarketDataUseCase:
    def __init__(self, market_repository: object) -> None:
        self._market_repository = market_repository

    async def run(self, ts_code: str, start_date: str, end_date: str) -> dict[str, object]:
        rows = await self._market_repository.fetch_raw(ts_code=ts_code, start_date=start_date, end_date=end_date)
        market = [_json_ready(asdict(row)) for row in rows["market"]]
        indicators = [_json_ready(asdict(row)) for row in rows["indicators"]]
        indexes = [_json_ready(asdict(row)) for row in rows["indexes"]]
        cyq_chips = [_json_ready(asdict(row)) for row in rows.get("cyq_chips", [])]
        cyq_perf = [_json_ready(asdict(row)) for row in rows.get("cyq_perf", [])]
        return {
            "query": {"ts_code": ts_code, "start_date": start_date, "end_date": end_date},
            "data": {
                "market": market,
                "indicators": indicators,
                "indexes": indexes,
                "cyq_chips": cyq_chips,
                "cyq_perf": cyq_perf,
            },
            "meta": {
                "row_count_market": len(market),
                "row_count_indicators": len(indicators),
                "row_count_indexes": len(indexes),
                "row_count_cyq_chips": len(cyq_chips),
                "row_count_cyq_perf": len(cyq_perf),
            },
        }
