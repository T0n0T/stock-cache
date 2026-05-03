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
    pre_close: float | None = None
    change: float | None = None
    pct_chg: float | None = None
    vol: float | None = None
    amount: float | None = None
    turnover_rate: float | None = None
    turnover_rate_f: float | None = None
    volume_ratio: float | None = None
    pe: float | None = None
    pe_ttm: float | None = None
    pb: float | None = None
    ps: float | None = None
    ps_ttm: float | None = None
    dv_ratio: float | None = None
    dv_ttm: float | None = None
    total_share: float | None = None
    float_share: float | None = None
    free_share: float | None = None
    total_mv: float | None = None
    circ_mv: float | None = None
    net_mf_vol: float | None = None
    net_mf_amount: float | None = None
    extra_market_jsonb: dict[str, object] = field(default_factory=dict)
    source_provider: str = "tushare"
    source_daily: str | None = None
    source_daily_basic: str | None = None
    source_moneyflow: str | None = None


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
    rsi_6: float | None = None
    rsi_12: float | None = None
    rsi_24: float | None = None
    boll_upper: float | None = None
    boll_mid: float | None = None
    boll_lower: float | None = None
    cci: float | None = None
    extra_factors_jsonb: dict[str, object] = field(default_factory=dict)
    source_provider: str = "tushare"
    source_interface: str = "stk_factor"
    calc_fallback_used: bool = False


@dataclass(slots=True)
class DailyIndexRow:
    ts_code: str
    trade_date: date
    name: str | None = None
    group_name: str | None = None
    open: float | None = None
    high: float | None = None
    low: float | None = None
    close: float | None = None
    pre_close: float | None = None
    change: float | None = None
    pct_chg: float | None = None
    vol: float | None = None
    amount: float | None = None
    pe: float | None = None
    pb: float | None = None
    float_mv: float | None = None
    total_mv: float | None = None
    extra_index_jsonb: dict[str, object] = field(default_factory=dict)
    source_provider: str = "tushare"
    source_daily: str | None = None
    source_basic: str | None = None


@dataclass(slots=True)
class JobRunSummary:
    job_id: str
    status: str
    started_at: str
    finished_at: str
    total_symbols: int
    success_symbols: list[str] = field(default_factory=list)
    failed_symbols: dict[str, str] = field(default_factory=dict)
