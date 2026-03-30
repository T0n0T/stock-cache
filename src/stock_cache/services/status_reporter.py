from pathlib import Path

from stock_cache.domain.models import JobRunSummary


class StatusReporter:
    def __init__(self, path: Path) -> None:
        self._path = path

    def write(self, summary: JobRunSummary) -> None:
        self._path.parent.mkdir(parents=True, exist_ok=True)
        lines = [
            f"job_id: {summary.job_id}",
            f"status: {summary.status}",
            f"started_at: {summary.started_at}",
            f"finished_at: {summary.finished_at}",
            f"total_symbols: {summary.total_symbols}",
            f"success_count: {len(summary.success_symbols)}",
            f"failed_count: {len(summary.failed_symbols)}",
            "",
            "successful_symbols:",
            *summary.success_symbols,
            "",
            "failed_symbols:",
            *[f"{code} | {reason}" for code, reason in summary.failed_symbols.items()],
        ]
        self._path.write_text("\n".join(lines) + "\n", encoding="utf-8")
