CREATE TABLE IF NOT EXISTS instruments (
  ts_code TEXT PRIMARY KEY,
  symbol TEXT NOT NULL,
  name TEXT NOT NULL,
  area TEXT,
  industry TEXT,
  market TEXT,
  exchange TEXT NOT NULL,
  list_status TEXT NOT NULL,
  list_date DATE,
  delist_date DATE,
  is_st BOOLEAN NOT NULL DEFAULT FALSE,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS daily_market (
  ts_code TEXT NOT NULL,
  trade_date DATE NOT NULL,
  open DOUBLE PRECISION,
  high DOUBLE PRECISION,
  low DOUBLE PRECISION,
  close DOUBLE PRECISION,
  pre_close DOUBLE PRECISION,
  change DOUBLE PRECISION,
  pct_chg DOUBLE PRECISION,
  vol DOUBLE PRECISION,
  amount DOUBLE PRECISION,
  turnover_rate DOUBLE PRECISION,
  turnover_rate_f DOUBLE PRECISION,
  volume_ratio DOUBLE PRECISION,
  pe DOUBLE PRECISION,
  pe_ttm DOUBLE PRECISION,
  pb DOUBLE PRECISION,
  ps DOUBLE PRECISION,
  ps_ttm DOUBLE PRECISION,
  dv_ratio DOUBLE PRECISION,
  dv_ttm DOUBLE PRECISION,
  total_share DOUBLE PRECISION,
  float_share DOUBLE PRECISION,
  free_share DOUBLE PRECISION,
  total_mv DOUBLE PRECISION,
  circ_mv DOUBLE PRECISION,
  net_mf_vol DOUBLE PRECISION,
  net_mf_amount DOUBLE PRECISION,
  extra_market_jsonb JSONB NOT NULL DEFAULT '{}'::jsonb,
  source_provider TEXT NOT NULL,
  source_daily TEXT,
  source_daily_basic TEXT,
  source_moneyflow TEXT,
  ingested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (ts_code, trade_date)
);

CREATE INDEX IF NOT EXISTS idx_daily_market_trade_date ON daily_market (trade_date);
CREATE INDEX IF NOT EXISTS idx_daily_market_trade_date_pct_chg ON daily_market (trade_date, pct_chg);
CREATE INDEX IF NOT EXISTS idx_daily_market_trade_date_turnover_rate ON daily_market (trade_date, turnover_rate);
CREATE INDEX IF NOT EXISTS idx_daily_market_trade_date_total_mv ON daily_market (trade_date, total_mv);

CREATE TABLE IF NOT EXISTS daily_indicators (
  ts_code TEXT NOT NULL,
  trade_date DATE NOT NULL,
  macd_dif DOUBLE PRECISION,
  macd_dea DOUBLE PRECISION,
  macd DOUBLE PRECISION,
  kdj_k DOUBLE PRECISION,
  kdj_d DOUBLE PRECISION,
  kdj_j DOUBLE PRECISION,
  rsi_6 DOUBLE PRECISION,
  rsi_12 DOUBLE PRECISION,
  rsi_24 DOUBLE PRECISION,
  boll_upper DOUBLE PRECISION,
  boll_mid DOUBLE PRECISION,
  boll_lower DOUBLE PRECISION,
  cci DOUBLE PRECISION,
  extra_factors_jsonb JSONB NOT NULL DEFAULT '{}'::jsonb,
  source_provider TEXT NOT NULL,
  source_interface TEXT NOT NULL,
  calc_fallback_used BOOLEAN NOT NULL DEFAULT FALSE,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (ts_code, trade_date)
);

CREATE TABLE IF NOT EXISTS daily_index (
  ts_code TEXT NOT NULL,
  trade_date DATE NOT NULL,
  name TEXT,
  group_name TEXT,
  open DOUBLE PRECISION,
  high DOUBLE PRECISION,
  low DOUBLE PRECISION,
  close DOUBLE PRECISION,
  pre_close DOUBLE PRECISION,
  change DOUBLE PRECISION,
  pct_chg DOUBLE PRECISION,
  vol DOUBLE PRECISION,
  amount DOUBLE PRECISION,
  pe DOUBLE PRECISION,
  pb DOUBLE PRECISION,
  float_mv DOUBLE PRECISION,
  total_mv DOUBLE PRECISION,
  extra_index_jsonb JSONB NOT NULL DEFAULT '{}'::jsonb,
  source_provider TEXT NOT NULL,
  source_daily TEXT,
  source_basic TEXT,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (ts_code, trade_date)
);

CREATE INDEX IF NOT EXISTS idx_daily_index_trade_date ON daily_index (trade_date);
CREATE INDEX IF NOT EXISTS idx_daily_index_group_name_trade_date ON daily_index (group_name, trade_date);

CREATE TABLE IF NOT EXISTS job_runs (
  job_id TEXT PRIMARY KEY,
  job_type TEXT NOT NULL,
  started_at TIMESTAMPTZ NOT NULL,
  finished_at TIMESTAMPTZ,
  status TEXT NOT NULL,
  total_symbols INTEGER NOT NULL,
  success_symbols INTEGER NOT NULL,
  failed_symbols INTEGER NOT NULL,
  status_file_path TEXT NOT NULL,
  error_summary TEXT,
  params_json JSONB NOT NULL DEFAULT '{}'::jsonb
);
