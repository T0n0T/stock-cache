from pathlib import Path

from domain.models import JobRunSummary
from services.status_reporter import StatusReporter


def test_status_reporter_overwrites_file(tmp_path: Path) -> None:
    status_file = tmp_path / "last-write-status.txt"
    reporter = StatusReporter(status_file)
    first_summary = JobRunSummary(
        job_id="20260330T120000Z",
        status="partial_success",
        started_at="2026-03-30T12:00:00Z",
        finished_at="2026-03-30T12:18:42Z",
        total_symbols=3,
        success_symbols=["000001.SZ", "000002.SZ"],
        failed_symbols={"600000.SH": "timeout after retries"},
    )
    second_summary = JobRunSummary(
        job_id="20260330T130000Z",
        status="success",
        started_at="2026-03-30T13:00:00Z",
        finished_at="2026-03-30T13:07:00Z",
        total_symbols=1,
        success_symbols=["300750.SZ"],
        failed_symbols={},
    )

    reporter.write(first_summary)
    reporter.write(second_summary)
    contents = status_file.read_text(encoding="utf-8")

    assert "300750.SZ" in contents
    assert "job_id: 20260330T130000Z" in contents
    assert "status: success" in contents
    assert "000001.SZ" not in contents
    assert "000002.SZ" not in contents
    assert "600000.SH | timeout after retries" not in contents
