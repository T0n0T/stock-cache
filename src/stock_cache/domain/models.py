from dataclasses import dataclass, field
from datetime import date


@dataclass(slots=True)
class Instrument:
    ts_code: str
    symbol: str
    name: str
    exchange: str
    list_status: str
    is_st: bool


@dataclass(slots=True)
class DailyMarketRow:
    ts_code: str
    trade_date: date
    open: float | None = None
    high: float | None = None
    low: float | None = None
    close: float | None = None
    pct_chg: float | None = None
    turnover_rate: float | None = None
    total_mv: float | None = None
    net_mf_amount: float | None = None
    source_provider: str = "tushare"


@dataclass(slots=True)
class DailyIndicatorRow:
    ts_code: str
    trade_date: date
    macd_dif: float | None = None
    macd_dea: float | None = None
    macd: float | None = None
    kdj_k: float | None = None
    kdj_d: float | None = None
    kdj_j: float | None = None
    source_provider: str = "tushare"
    source_interface: str = "stk_factor"
    calc_fallback_used: bool = False


@dataclass(slots=True)
class JobRunSummary:
    job_id: str
    status: str
    started_at: str
    finished_at: str
    total_symbols: int
    success_symbols: list[str] = field(default_factory=list)
    failed_symbols: dict[str, str] = field(default_factory=dict)
