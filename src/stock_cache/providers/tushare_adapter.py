class TushareAdapter:
    def fetch_daily(self, ts_code: str, start_date: str, end_date: str) -> list[dict[str, object]]:
        raise NotImplementedError("TushareAdapter.fetch_daily is implemented in Task 10")
