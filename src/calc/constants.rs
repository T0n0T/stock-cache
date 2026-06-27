pub(super) const MACD_CALC_VERSION: &str = "macd_qfq_12_26_9_v1";
pub(super) const ROLLING_CALC_VERSION: &str = "rolling_qfq_v1";
pub(super) const LEFT_PEAK_CALC_VERSION: &str = "left_peak_qfq_v1";

pub(super) const ALPHA12: f64 = 2.0 / (12.0 + 1.0);
pub(super) const ALPHA26: f64 = 2.0 / (26.0 + 1.0);
pub(super) const ALPHA9: f64 = 2.0 / (9.0 + 1.0);

pub(super) const SWING_LOOKBACK: usize = 2;
pub(super) const BREAKOUT_LOOKBACK: usize = 5;
pub(super) const HIGH_CYCLE_DAYS: usize = 360;
pub(super) const MIN_PULLBACK_PCT: f64 = 5.0;
