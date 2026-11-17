use anyhow::{bail, Result};

use super::{
    model::{Bi, CzscBar, CzscConfig, CzscSnapshot, CzscSourceBar, Direction, Fx, Mark, NewBar},
    structure::build_snapshot,
};

#[derive(Debug)]
pub(crate) struct CzscAnalyzer {
    config: CzscConfig,
    last_source_date: Option<chrono::NaiveDate>,
    raw_bars: Vec<CzscBar>,
    bars_ubi: Vec<NewBar>,
    bi_list: Vec<Bi>,
}

impl CzscAnalyzer {
    pub(crate) fn new(config: CzscConfig) -> Self {
        Self {
            config,
            last_source_date: None,
            raw_bars: Vec::new(),
            bars_ubi: Vec::new(),
            bi_list: Vec::new(),
        }
    }

    #[cfg(test)]
    pub(in crate::calc) fn update(&mut self, bar: CzscBar) -> Result<()> {
        if !bar.is_valid() {
            bail!("invalid OHLC on {}", bar.trade_date);
        }
        self.check_source_date(bar.trade_date)?;
        self.update_valid(bar);
        self.last_source_date = Some(bar.trade_date);
        Ok(())
    }

    pub(crate) fn process(&mut self, source: CzscSourceBar) -> Result<CzscSnapshot> {
        self.check_source_date(source.trade_date)?;
        let valid = source.valid_bar();
        if let Some(bar) = valid {
            self.update_valid(bar);
        }
        self.last_source_date = Some(source.trade_date);
        Ok(build_snapshot(self, valid.is_some()))
    }

    fn check_source_date(&self, trade_date: chrono::NaiveDate) -> Result<()> {
        if let Some(previous) = self.last_source_date {
            if trade_date <= previous {
                bail!(
                    "CZSC bars must be strictly increasing: {} after {}",
                    trade_date,
                    previous
                );
            }
        }
        Ok(())
    }

    fn update_valid(&mut self, bar: CzscBar) {
        let raw_index = self.raw_bars.len();
        self.raw_bars.push(bar);
        if self.bars_ubi.len() < 2 {
            self.bars_ubi.push(NewBar::from_raw(raw_index, bar));
        } else {
            let length = self.bars_ubi.len();
            let next = remove_include(
                &self.bars_ubi[length - 2],
                &self.bars_ubi[length - 1],
                raw_index,
                bar,
            );
            if next.included {
                self.bars_ubi[length - 1] = next.bar;
            } else {
                self.bars_ubi.push(next.bar);
            }
        }

        self.update_bi();
        if self.bi_list.len() > self.config.max_bi_num {
            let drain_to = self.bi_list.len() - self.config.max_bi_num;
            self.bi_list.drain(0..drain_to);
        }
    }

    #[cfg(test)]
    pub(super) fn bi_list(&self) -> Vec<super::model::BiSummary> {
        self.bi_list.iter().map(Bi::summary).collect()
    }

    #[cfg(test)]
    pub(super) fn finished_bis(&self) -> Vec<super::model::BiSummary> {
        self.finished_bis_internal()
            .iter()
            .map(|bi| bi.summary())
            .collect()
    }

    pub(super) fn raw_bars(&self) -> &[CzscBar] {
        &self.raw_bars
    }

    pub(super) fn finished_bis_internal(&self) -> &[Bi] {
        if self.bi_list.is_empty() {
            return &[];
        }
        if self.bars_ubi.len() < 5 {
            return &self.bi_list[..self.bi_list.len().saturating_sub(1)];
        }
        &self.bi_list
    }

    fn update_bi(&mut self) {
        if self.bars_ubi.len() < 3 {
            return;
        }

        if self.bi_list.is_empty() {
            let fxs = check_fxs(&self.bars_ubi);
            let Some(first) = fxs.first() else {
                return;
            };
            let mut fx_a = first;
            for candidate in fxs.iter().filter(|fx| fx.mark == first.mark) {
                match first.mark {
                    Mark::Bottom if candidate.low <= fx_a.low => fx_a = candidate,
                    Mark::Top if candidate.high >= fx_a.high => fx_a = candidate,
                    _ => {}
                }
            }
            let candidate_bars = self
                .bars_ubi
                .iter()
                .filter(|bar| bar.trade_date >= fx_a.first_date)
                .cloned()
                .collect::<Vec<_>>();
            let checked = check_bi(&candidate_bars, self.config.min_bi_len);
            if let Some(bi) = checked.bi {
                self.bi_list.push(bi);
            }
            self.bars_ubi = checked.remaining;
            return;
        }

        let checked = check_bi(&self.bars_ubi, self.config.min_bi_len);
        if let Some(bi) = checked.bi {
            self.bi_list.push(bi);
        }
        self.bars_ubi = checked.remaining;

        let last = self.bi_list.last().expect("non-empty BI list");
        let broken = self
            .bars_ubi
            .last()
            .is_some_and(|bar| match last.direction {
                Direction::Up => bar.high > last.high(),
                Direction::Down => bar.low < last.low(),
            });
        if broken {
            let last = self.bi_list.pop().expect("last BI exists");
            let merge_point = last.bars[last.bars.len() - 2].trade_date;
            self.bars_ubi = last.bars[..last.bars.len() - 2]
                .iter()
                .chain(
                    self.bars_ubi
                        .iter()
                        .filter(|bar| bar.trade_date >= merge_point),
                )
                .cloned()
                .collect();
        }
    }
}

struct IncludedBar {
    included: bool,
    bar: NewBar,
}

fn remove_include(k1: &NewBar, k2: &NewBar, raw_index: usize, k3: CzscBar) -> IncludedBar {
    let direction = if k1.high < k2.high {
        Some(Direction::Up)
    } else if k1.high > k2.high {
        Some(Direction::Down)
    } else {
        None
    };
    let Some(direction) = direction else {
        return IncludedBar {
            included: false,
            bar: NewBar::from_raw(raw_index, k3),
        };
    };
    let included =
        (k2.high <= k3.high && k2.low >= k3.low) || (k2.high >= k3.high && k2.low <= k3.low);
    if !included {
        return IncludedBar {
            included: false,
            bar: NewBar::from_raw(raw_index, k3),
        };
    }

    let (high, low, trade_date) = match direction {
        Direction::Up => (
            k2.high.max(k3.high),
            k2.low.max(k3.low),
            if k2.high > k3.high {
                k2.trade_date
            } else {
                k3.trade_date
            },
        ),
        Direction::Down => (
            k2.high.min(k3.high),
            k2.low.min(k3.low),
            if k2.low < k3.low {
                k2.trade_date
            } else {
                k3.trade_date
            },
        ),
    };
    let mut raw_indices = k2
        .raw_indices
        .iter()
        .take(100)
        .copied()
        .filter(|index| *index != raw_index)
        .collect::<Vec<_>>();
    raw_indices.push(raw_index);
    IncludedBar {
        included: true,
        bar: NewBar {
            trade_date,
            high,
            low,
            raw_indices,
        },
    }
}

fn check_fxs(bars: &[NewBar]) -> Vec<Fx> {
    let mut fxs = Vec::new();
    for window in bars.windows(3) {
        if let Some(fx) = check_fx(&window[0], &window[1], &window[2]) {
            if fxs.len() >= 2 && fxs.last().is_some_and(|last: &Fx| last.mark == fx.mark) {
                continue;
            }
            fxs.push(fx);
        }
    }
    fxs
}

fn check_fx(k1: &NewBar, k2: &NewBar, k3: &NewBar) -> Option<Fx> {
    if k1.high < k2.high && k2.high > k3.high && k1.low < k2.low && k2.low > k3.low {
        return Some(Fx {
            trade_date: k2.trade_date,
            mark: Mark::Top,
            high: k2.high,
            low: k2.low,
            value: k2.high,
            first_date: k1.trade_date,
            last_date: k3.trade_date,
        });
    }
    if k1.low > k2.low && k2.low < k3.low && k1.high > k2.high && k2.high < k3.high {
        return Some(Fx {
            trade_date: k2.trade_date,
            mark: Mark::Bottom,
            high: k2.high,
            low: k2.low,
            value: k2.low,
            first_date: k1.trade_date,
            last_date: k3.trade_date,
        });
    }
    None
}

struct CheckedBi {
    bi: Option<Bi>,
    remaining: Vec<NewBar>,
}

fn check_bi(bars: &[NewBar], min_bi_len: usize) -> CheckedBi {
    let fxs = check_fxs(bars);
    if fxs.len() < 2 {
        return CheckedBi {
            bi: None,
            remaining: bars.to_vec(),
        };
    }
    let fx_a = &fxs[0];
    let (direction, fx_b) = match fx_a.mark {
        Mark::Bottom => {
            let mut best: Option<&Fx> = None;
            for candidate in fxs.iter().filter(|fx| {
                fx.mark == Mark::Top && fx.trade_date > fx_a.trade_date && fx.value > fx_a.value
            }) {
                if best.is_none_or(|current| candidate.high > current.high) {
                    best = Some(candidate);
                }
            }
            (Direction::Up, best)
        }
        Mark::Top => {
            let mut best: Option<&Fx> = None;
            for candidate in fxs.iter().filter(|fx| {
                fx.mark == Mark::Bottom && fx.trade_date > fx_a.trade_date && fx.value < fx_a.value
            }) {
                if best.is_none_or(|current| candidate.low < current.low) {
                    best = Some(candidate);
                }
            }
            (Direction::Down, best)
        }
    };
    let Some(fx_b) = fx_b else {
        return CheckedBi {
            bi: None,
            remaining: bars.to_vec(),
        };
    };

    let start_idx = bars.partition_point(|bar| bar.trade_date < fx_a.first_date);
    let end_idx = bars.partition_point(|bar| bar.trade_date <= fx_b.last_date);
    if start_idx >= end_idx {
        return CheckedBi {
            bi: None,
            remaining: bars.to_vec(),
        };
    }
    let bars_a = &bars[start_idx..end_idx];
    let new_start_idx = bars.partition_point(|bar| bar.trade_date < fx_b.first_date);
    let remaining = bars[new_start_idx..].to_vec();
    let endpoints_include = (fx_a.high > fx_b.high && fx_a.low < fx_b.low)
        || (fx_a.high < fx_b.high && fx_a.low > fx_b.low);
    if endpoints_include || bars_a.len() < min_bi_len {
        return CheckedBi {
            bi: None,
            remaining: bars.to_vec(),
        };
    }

    let fx_a = fx_a.clone();
    let fx_b = fx_b.clone();
    CheckedBi {
        bi: Some(Bi {
            fx_a,
            fx_b,
            direction,
            bars: bars_a.to_vec(),
        }),
        remaining,
    }
}
