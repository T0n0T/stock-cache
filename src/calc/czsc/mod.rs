mod analyzer;
mod model;
mod multi_period;
mod structure;

pub(crate) use analyzer::CzscAnalyzer;
#[cfg(test)]
pub(crate) use model::CzscSnapshot as AnalyzerSnapshot;
pub(crate) use model::{CzscConfig, CzscSourceBar};
pub use multi_period::{
    calculate_czsc, calculate_multi_period_czsc, CzscSnapshot, MultiPeriodCzscRow, OhlcBar,
};

#[cfg(test)]
pub(super) use model::CzscBar;

#[cfg(test)]
mod tests;
