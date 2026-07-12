# CZSC source and golden baseline

This directory records the upstream source for a future, minimal CZSC port. Batch A
contains documentation and test fixtures only; it does not contain an analyzer or
database integration.

## Pinned source

- Project: CZSC, <https://github.com/waditu/czsc>
- Commit: `9ab62854f6bfab8515115b942baf4deb0f06185c`
- Version at that commit: `1.0.0-rc.8`
- Git description supplied by the implementation handoff:
  `v1.0.0-rc.8-16-g9ab6285`
- Upstream copyright: Copyright 2025 zengbin93
- License for the selected source: Apache License 2.0. See
  [`THIRD_PARTY_LICENSES/CZSC-APACHE-2.0.txt`](../../../THIRD_PARTY_LICENSES/CZSC-APACHE-2.0.txt)
  and [`NOTICE`](../../../NOTICE).

The CZSC workspace manifest at the pinned commit declares `MIT`, while its root
`LICENSE` contains the Apache-2.0 text. This port follows the supplied upstream
license file and implementation handoff: Apache-2.0. The manifest discrepancy is
recorded here rather than silently choosing a different license.

## Reviewed algorithm source

The Batch A baseline was prepared from these files at the pinned commit:

- `crates/czsc-core/src/analyze/mod.rs`
- `crates/czsc-core/src/analyze/utils.rs`
- `crates/czsc-core/src/objects/bar.rs`
- `crates/czsc-core/src/objects/fx.rs`
- `crates/czsc-core/src/objects/bi.rs`
- `crates/czsc-core/src/objects/zs.rs`
- `crates/czsc-core/src/objects/direction.rs`
- `crates/czsc-core/src/objects/mark.rs`
- `crates/czsc-signals/src/utils/cxt.rs` (`get_zs_seq`)

## Planned minimum port

Later batches may port only the deterministic daily-price structure path:

- raw and inclusion-removed K-line representations with source-index mapping;
- inclusion removal, fractals, BI creation, last-BI extension and removal;
- `finished_bis`, capped at 50 BI with `min_bi_len = 6`;
- `get_zs_seq` and the minimal ZS overlap facts needed by stock-cache;
- point-in-time daily snapshots from qfq OHLC.

The port will not include Python or PyO3 bindings, Polars/DataFrame conversion,
plotting, signal registries or signal functions, traders, positions, technical or
volume-price indicators, caches, resampling, volume, or amount. Volume and amount
are deliberately absent from the fixture format.

## Golden fixture contract

The fixtures live in `tests/fixtures/czsc` and are ordinary JSON. Each file is
self-describing and contains:

- `schema_version`;
- pinned source metadata (`source_czsc_commit`, version, license,
  `min_bi_len = 6`, `max_bi_num = 50`, `price_mode = qfq`);
- cases with anonymous IDs, coverage tags, daily qfq OHLC input, and one expected
  state for every input day;
- expected inclusion-removed bar count, full internal `bi_list`, `finished_bis`,
  `get_zs_seq`, and observed last-BI/tail events.

`last_bi_extension` denotes the pinned analyzer's observable lifecycle: an
existing last BI is broken and removed, then later rebuilt from the same start
fractal with a later or more extreme end fractal. The fixtures retain the full
daily `bi_list` so this sequence is directly verifiable rather than inferred
from a coverage label.

Dates, directions, and counts are exact. Later parity tests should compare prices
with tolerance `1e-9`. Invalid OHLC days have `source_bar_valid = false`; they are
not passed to CZSC, and their expected state is the previous valid day's state.
This is the frozen stock-cache entry rule, not behavior implemented inside CZSC.

`anonymized_real_daily.json` contains 160 consecutive daily observations from a
public A-share series fetched from Eastmoney's K-line endpoint with `fqt=1`
(qfq). The symbol and name were removed before the fixture was written. Only date
and qfq OHLC remain. `handcrafted_daily.json` covers the specified edge cases.

The committed JSON files are the authoritative, standalone test assets. Normal
builds and tests do not require network access, Python, the CZSC checkout, or the
fixture generator. Maintainers can regenerate them explicitly with:

```bash
/path/to/czsc/.venv/bin/python tools/generate_czsc_golden.py \
  --czsc-repo /path/to/czsc \
  --real-source /path/to/eastmoney-qfq-response.json
```

The generator refuses a CZSC checkout whose HEAD or package version does not
match the pinned baseline. `--real-source` must be an Eastmoney K-line response
obtained with `fqt=1`; it is deliberately supplied out of band so the committed
repository does not retain a security identifier. Path arguments are local CLI
inputs; no absolute machine path is stored in fixtures or tests.
