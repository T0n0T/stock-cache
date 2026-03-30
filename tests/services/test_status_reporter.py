from pathlib import Path

from stock_cache.domain.models import JobRunSummary
from stock_cache.services.status_reporter import StatusReporter


def test_status_reporter_overwrites_file(tmp_path: Path) -> None:
    status_file = tmp_path / "last-write-status.txt"
    reporter = StatusReporter(status_file)
    summary = JobRunSummary(
        job_id="20260330T120000Z",
        status="partial_success",
        started_at="2026-03-30T12:00:00Z",
        finished_at="2026-03-30T12:18:42Z",
        total_symbols=3,
        success_symbols=["000001.SZ", "000002.SZ"],
        failed_symbols={"600000.SH": "timeout after retries"},
    )

    reporter.write(summary)
    contents = status_file.read_text(encoding="utf-8")

    assert "000001.SZ" in contents
    assert "600000.SH | timeout after retries" in contents
