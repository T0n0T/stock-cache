use chrono::NaiveDate;

use super::{
    analyzer::CzscAnalyzer,
    model::{Bi, CzscBar, CzscSnapshot, Direction},
};

pub(super) fn build_snapshot(analyzer: &CzscAnalyzer, source_bar_valid: bool) -> CzscSnapshot {
    let finished = analyzer.finished_bis_internal();
    let recent_bi_count = finished.len().min(7);
    let recent = &finished[finished.len().saturating_sub(recent_bi_count)..];
    let last = finished.last();
    let latest_zs = get_zs_seq(finished)
        .into_iter()
        .rev()
        .find(|zs| zs.bi_indices.len() >= 3 && zs.is_valid(finished));
    let peak = recent_peak(analyzer.raw_bars(), recent);
    let bear = peak.and_then(|(peak_date, _)| {
        analyzer
            .raw_bars()
            .iter()
            .find(|bar| bar.trade_date > peak_date && bar.close < bar.open)
    });
    let status = if !source_bar_valid {
        "invalid_source_bar"
    } else if finished.is_empty() {
        "no_finished_bi"
    } else if finished.len() < 3 {
        "insufficient_bis_for_zs"
    } else if latest_zs.is_none() {
        "no_valid_zs"
    } else {
        "ok"
    };
    let last_high = last.map(Bi::high);
    let last_low = last.map(Bi::low);

    CzscSnapshot {
        source_bar_valid,
        status: status.to_string(),
        available_finished_bi_count: finished.len(),
        recent_bi_count,
        last_bi_direction: last.map(|bi| bi.direction.as_str().to_string()),
        last_bi_start_date: last.map(|bi| bi.fx_a.trade_date),
        last_bi_end_date: last.map(|bi| bi.fx_b.trade_date),
        last_bi_high: last_high,
        last_bi_low: last_low,
        last_bi_mid: last_high
            .zip(last_low)
            .map(|(high, low)| (high + low) / 2.0),
        latest_zs_start_date: latest_zs.as_ref().map(|zs| zs.start_date),
        latest_zs_end_date: latest_zs.as_ref().map(|zs| zs.end_date),
        latest_zs_bi_count: latest_zs.as_ref().map(|zs| zs.bi_indices.len()),
        latest_zs_zg: latest_zs.as_ref().map(|zs| zs.zg),
        latest_zs_zd: latest_zs.as_ref().map(|zs| zs.zd),
        latest_zs_zz: latest_zs.as_ref().map(|zs| zs.zz),
        recent_peak_date: peak.map(|value| value.0),
        recent_peak_high: peak.map(|value| value.1),
        post_peak_first_bear_date: bear.map(|bar| bar.trade_date),
        post_peak_first_bear_open: bear.map(|bar| bar.open),
        post_peak_first_bear_high: bear.map(|bar| bar.high),
        post_peak_first_bear_low: bear.map(|bar| bar.low),
        post_peak_first_bear_close: bear.map(|bar| bar.close),
        post_peak_first_bear_missing: peak.map(|_| bear.is_none()),
    }
}

#[derive(Debug)]
struct Zs {
    bi_indices: Vec<usize>,
    start_date: NaiveDate,
    end_date: NaiveDate,
    zg: f64,
    zd: f64,
    zz: f64,
}

impl Zs {
    fn new(finished: &[Bi], bi_index: usize) -> Self {
        let bi = &finished[bi_index];
        Self {
            bi_indices: vec![bi_index],
            start_date: bi.fx_a.trade_date,
            end_date: bi.fx_b.trade_date,
            zg: bi.high(),
            zd: bi.low(),
            zz: (bi.high() + bi.low()) / 2.0,
        }
    }

    fn push(&mut self, finished: &[Bi], bi_index: usize) {
        self.bi_indices.push(bi_index);
        self.end_date = finished[bi_index].fx_b.trade_date;
        let first_three = self
            .bi_indices
            .iter()
            .take(3)
            .map(|index| &finished[*index]);
        self.zg = first_three
            .clone()
            .map(Bi::high)
            .fold(f64::INFINITY, f64::min);
        self.zd = first_three.map(Bi::low).fold(f64::NEG_INFINITY, f64::max);
        self.zz = (self.zg + self.zd) / 2.0;
    }

    fn is_valid(&self, finished: &[Bi]) -> bool {
        self.zg >= self.zd
            && self.bi_indices.iter().all(|index| {
                let bi = &finished[*index];
                let high_in_range = bi.high() <= self.zg && bi.high() >= self.zd;
                let low_in_range = bi.low() <= self.zg && bi.low() >= self.zd;
                let contains_range = bi.high() >= self.zg && bi.low() <= self.zd;
                high_in_range || low_in_range || contains_range
            })
    }
}

fn get_zs_seq(finished: &[Bi]) -> Vec<Zs> {
    let mut sequence: Vec<Zs> = Vec::new();
    for (index, bi) in finished.iter().enumerate() {
        let Some(last) = sequence.last_mut() else {
            sequence.push(Zs::new(finished, index));
            continue;
        };
        let separated = (bi.direction == Direction::Up && bi.high() < last.zd)
            || (bi.direction == Direction::Down && bi.low() > last.zg);
        if separated {
            sequence.push(Zs::new(finished, index));
        } else {
            last.push(finished, index);
        }
    }
    sequence
}

fn recent_peak(raw_bars: &[CzscBar], recent: &[Bi]) -> Option<(NaiveDate, f64)> {
    let mut peak: Option<(NaiveDate, f64)> = None;
    for bi in recent {
        if bi.bars.len() <= 2 {
            continue;
        }
        for new_bar in &bi.bars[1..bi.bars.len() - 1] {
            for raw_index in &new_bar.raw_indices {
                let raw = &raw_bars[*raw_index];
                if peak.is_none_or(|(date, high)| {
                    raw.high > high || (raw.high == high && raw.trade_date > date)
                }) {
                    peak = Some((raw.trade_date, raw.high));
                }
            }
        }
    }
    peak
}
