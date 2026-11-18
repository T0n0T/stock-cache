//! Internal calculation and batch persistence used by the market-cache workflow.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{self, BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use stock_cache::{
    calc::{checkpoint, czsc, kdj, left_peak, macd, rolling},
    storage::duckdb,
};

#[derive(Debug)]
struct Args {
    input: String,
    calendar: PathBuf,
    source_dataset: Option<String>,
    /// Source date at which a correction starts; with --write, use persisted
    /// checkpoints and warmup windows to limit the replacement slice.
    changed_from: Option<String>,
    calc_version: String,
    price_basis: String,
    kdj_window: usize,
    output: String,
    /// Persist all calculated tables through the Rust DuckDB writer.
    database: Option<PathBuf>,
    write: bool,
    scope: Vec<String>,
    output_from: Option<NaiveDate>,
    output_to: Option<NaiveDate>,
}

#[derive(Debug, Deserialize)]
struct InputRow {
    source_dataset: Option<String>,
    ts_code: String,
    trade_date: String,
    open: Option<Value>,
    high: Option<Value>,
    low: Option<Value>,
    close: Option<Value>,
    vol: Option<Value>,
    amount: Option<Value>,
    turnover_rate: Option<Value>,
}

#[derive(Debug, Clone)]
struct Bar {
    source_dataset: String,
    ts_code: String,
    trade_date: NaiveDate,
    open: Option<f64>,
    high: Option<f64>,
    low: Option<f64>,
    close: Option<f64>,
    vol: Option<f64>,
    amount: Option<f64>,
    turnover_rate: Option<f64>,
}

#[derive(Debug, Serialize)]
struct OutputRow {
    table: &'static str,
    row: Value,
}

fn run(args: Args) -> Result<()> {
    let calendar = read_calendar(&args.calendar)?;
    let bars = read_input(&args)?;
    let mut groups: BTreeMap<(String, String), Vec<Bar>> = BTreeMap::new();
    for bar in bars {
        groups
            .entry((bar.source_dataset.clone(), bar.ts_code.clone()))
            .or_default()
            .push(bar);
    }
    if args.write {
        let changed_from = args.changed_from.as_deref().map(parse_date).transpose()?;
        if let Some(changed_from) = changed_from {
            validate_incremental_inputs(
                args.database
                    .as_deref()
                    .expect("cached calculation requires database"),
                &calendar,
                &groups,
                changed_from,
                &args.calc_version,
            )?;
        }
        let mut rendered = Vec::new();
        calculate_groups(&args, &calendar, groups, &mut rendered)?;
        eprintln!("[write] persisting calculated batch");
        let scope = parse_scope(&args.scope)?;
        persist_calculation(
            args.database
                .as_deref()
                .expect("cached calculation requires database"),
            &calendar,
            changed_from,
            &rendered,
            &scope,
        )?;
        write_output(&args.output, &rendered)?;
    } else {
        let output: Box<dyn Write> = if args.output == "-" {
            Box::new(BufWriter::new(io::stdout().lock()))
        } else {
            Box::new(BufWriter::new(File::create(&args.output).with_context(
                || format!("failed to create output {}", args.output),
            )?))
        };
        let mut output = output;
        calculate_groups(&args, &calendar, groups, &mut output)?;
        output.flush()?;
    }
    Ok(())
}

fn validate_incremental_inputs(
    database: &Path,
    calendar: &[NaiveDate],
    groups: &BTreeMap<(String, String), Vec<Bar>>,
    changed_from: NaiveDate,
    calc_version: &str,
) -> Result<()> {
    duckdb::with_connection(&database.to_string_lossy(), |connection| {
        duckdb::create_schema(connection)?;
        for ((source_dataset, ts_code), bars) in groups {
            let checkpoints = duckdb::load_indicator_checkpoints(
                connection,
                source_dataset,
                ts_code,
                calc_version,
            )?;
            let plan = checkpoint::plan_recompute(changed_from, calendar, &checkpoints)?;
            let first = bars
                .first()
                .map(|bar| bar.trade_date)
                .ok_or_else(|| anyhow!("incremental group {source_dataset}/{ts_code} is empty"))?;
            if first > plan.output_from {
                bail!(
                    "incremental input for {source_dataset}/{ts_code} starts at {first}, but recomputation requires warmup from {}",
                    plan.output_from
                );
            }
        }
        Ok(())
    })
}

fn calculate_groups<W: Write>(
    args: &Args,
    calendar: &[NaiveDate],
    groups: BTreeMap<(String, String), Vec<Bar>>,
    output: &mut W,
) -> Result<()> {
    let total = groups.len();
    let mut last_progress = std::time::Instant::now();
    for (index, ((_dataset, _ts_code), bars)) in groups.into_iter().enumerate() {
        calculate_group(args, calendar, &bars, output)?;
        if index + 1 < total && last_progress.elapsed().as_secs() >= 5 {
            eprintln!(
                "[write] calculating batch: {}/{total} instruments",
                index + 1
            );
            last_progress = std::time::Instant::now();
        }
    }
    Ok(())
}

fn write_output(path: &str, rendered: &[u8]) -> Result<()> {
    if path == "-" {
        io::stdout().write_all(rendered)?;
    } else {
        fs::write(path, rendered).with_context(|| format!("failed to create output {path}"))?;
    }
    Ok(())
}

fn parse_scope(values: &[String]) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    for value in values {
        let Some((key, raw)) = value.split_once('=') else {
            bail!("scope must be key=value: {value}");
        };
        result.insert(key.to_string(), raw.to_string());
    }
    Ok(result)
}

fn persist_calculation(
    database: &Path,
    calendar: &[NaiveDate],
    requested_changed_from: Option<NaiveDate>,
    rendered: &[u8],
    extra_scope: &BTreeMap<String, String>,
) -> Result<()> {
    let aggregate_by_table = requested_changed_from.is_none();
    let mut groups: BTreeMap<(String, String, String, String, String), Vec<Value>> =
        BTreeMap::new();
    for (line_number, line) in rendered.split(|byte| *byte == b'\n').enumerate() {
        if line.is_empty() {
            continue;
        }
        let envelope: Value = serde_json::from_slice(line)
            .with_context(|| format!("invalid calculated output at line {}", line_number + 1))?;
        let table = envelope
            .get("table")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("calculated output line {} has no table", line_number + 1))?;
        let row = envelope
            .get("row")
            .cloned()
            .ok_or_else(|| anyhow!("calculated output line {} has no row", line_number + 1))?;
        let source_dataset = row
            .get("source_dataset")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("calculated row has no source_dataset"))?;
        let ts_code = row
            .get("ts_code")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("calculated row has no ts_code"))?;
        let calc_version = row
            .get("calc_version")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("calculated row has no calc_version"))?;
        groups
            .entry((
                table.to_string(),
                source_dataset.to_string(),
                calc_version.to_string(),
                if aggregate_by_table {
                    String::new()
                } else {
                    ts_code.to_string()
                },
                row.get("period_type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            ))
            .or_default()
            .push(row);
    }
    let temp_root = std::env::temp_dir().join(format!(
        "stock-cache-calc-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or_default()
    ));
    fs::create_dir_all(&temp_root)?;
    let result = duckdb::with_connection(&database.to_string_lossy(), |connection| {
        duckdb::create_schema(connection)?;
        for (index, ((table, source_dataset, calc_version, ts_code, period_type), rows)) in
            groups.into_iter().enumerate()
        {
            let date_key = if matches!(table.as_str(), "calc_period_bars" | "calc_indicator_state")
            {
                "period_end_date"
            } else {
                "trade_date"
            };
            let changed_from = requested_changed_from;
            let checkpoints = if ts_code.is_empty() {
                Vec::new()
            } else {
                duckdb::load_indicator_checkpoints(
                    connection,
                    &source_dataset,
                    &ts_code,
                    &calc_version,
                )?
            };
            let recompute_from = changed_from
                .and_then(|changed| {
                    checkpoint::plan_recompute(changed, calendar, &checkpoints).ok()
                })
                .map(|plan| match table.as_str() {
                    "calc_daily_indicators" | "calc_period_bars" | "calc_indicator_state" => {
                        plan.macd_from
                    }
                    "calc_daily_rolling_factors" => plan.rolling_from,
                    "calc_daily_left_peak" => plan.left_peak_from,
                    _ => plan.czsc_from,
                });
            let rows = rows
                .into_iter()
                .filter(|row| {
                    recompute_from.is_none_or(|from| {
                        row.get(date_key)
                            .and_then(Value::as_str)
                            .and_then(|date| parse_date(date).ok())
                            .is_some_and(|date| date >= from)
                    })
                })
                .collect::<Vec<_>>();
            if rows.is_empty() {
                continue;
            }
            let path = temp_root.join(format!("{index}.jsonl"));
            let mut file = BufWriter::new(File::create(&path)?);
            for row in &rows {
                serde_json::to_writer(&mut file, row)?;
                file.write_all(b"\n")?;
            }
            file.flush()?;
            drop(file);
            let dates = rows
                .iter()
                .filter_map(|row| row.get(date_key).and_then(Value::as_str))
                .map(str::to_string)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            if dates.is_empty() {
                bail!("calculated table {table} has no {date_key}");
            }
            let mut scope = BTreeMap::from([
                ("source_dataset".to_string(), source_dataset),
                ("calc_version".to_string(), calc_version),
            ]);
            if !ts_code.is_empty() {
                scope.insert("ts_code".to_string(), ts_code);
            }
            if !period_type.is_empty() {
                scope.insert("period_type".to_string(), period_type);
            }
            for (key, value) in extra_scope {
                if let Some(existing) = scope.get(key) {
                    if existing != value {
                        bail!("scope {key}={value} conflicts with calculated row value {existing}");
                    }
                } else {
                    scope.insert(key.clone(), value.clone());
                }
            }
            duckdb::write_jsonl(
                connection,
                &table,
                &path,
                if aggregate_by_table {
                    "replace-batch-keys"
                } else {
                    "replace-dates"
                },
                &dates,
                &scope,
                &[],
            )?;
            fs::remove_file(path)?;
        }
        duckdb::checkpoint(connection)
    });
    let _ = fs::remove_dir_all(&temp_root);
    result
}

fn parse_date(value: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(value, "%Y%m%d"))
        .map_err(|_| anyhow!("invalid date: {value}"))
}

fn read_input(args: &Args) -> Result<Vec<Bar>> {
    let reader: Box<dyn BufRead> = if args.input == "-" {
        Box::new(BufReader::new(io::stdin().lock()))
    } else {
        Box::new(BufReader::new(File::open(&args.input).with_context(
            || format!("failed to open input {}", args.input),
        )?))
    };
    let mut bars = Vec::new();
    for (line, text) in reader.lines().enumerate() {
        let text = text?;
        if text.trim().is_empty() {
            continue;
        }
        let row: InputRow = serde_json::from_str(&text)
            .with_context(|| format!("invalid input JSONL at line {}", line + 1))?;
        let source_dataset = row
            .source_dataset
            .or_else(|| args.source_dataset.clone())
            .ok_or_else(|| anyhow!("source_dataset is required in input or --source-dataset"))?;
        bars.push(Bar {
            source_dataset,
            ts_code: row.ts_code,
            trade_date: parse_date(&row.trade_date)?,
            open: parse_number(row.open.as_ref())?,
            high: parse_number(row.high.as_ref())?,
            low: parse_number(row.low.as_ref())?,
            close: parse_number(row.close.as_ref())?,
            vol: parse_number(row.vol.as_ref())?,
            amount: parse_number(row.amount.as_ref())?,
            turnover_rate: parse_number(row.turnover_rate.as_ref())?,
        });
    }
    Ok(bars)
}

fn read_calendar(path: &PathBuf) -> Result<Vec<NaiveDate>> {
    let file =
        File::open(path).with_context(|| format!("failed to open calendar {}", path.display()))?;
    let mut dates = Vec::new();
    for (line, text) in BufReader::new(file).lines().enumerate() {
        let text = text?;
        if text.trim().is_empty() {
            continue;
        }
        let raw = text.trim().trim_matches('"');
        dates.push(
            parse_date(raw)
                .with_context(|| format!("invalid calendar date at line {}", line + 1))?,
        );
    }
    if dates.windows(2).any(|pair| pair[0] >= pair[1]) {
        bail!("calendar must be strictly increasing");
    }
    Ok(dates)
}

fn calculate_group<W: Write>(
    args: &Args,
    calendar: &[NaiveDate],
    bars: &[Bar],
    output: &mut W,
) -> Result<()> {
    if bars.is_empty() {
        return Ok(());
    }
    if bars
        .windows(2)
        .any(|pair| pair[0].trade_date >= pair[1].trade_date)
    {
        bail!("bars for one instrument must be strictly increasing");
    }
    let dataset = &bars[0].source_dataset;
    let ts_code = &bars[0].ts_code;
    let macd_bars = bars
        .iter()
        .map(|bar| macd::DailyClose {
            trade_date: bar.trade_date,
            close: bar.close,
        })
        .collect::<Vec<_>>();
    let ohlcv_bars = bars
        .iter()
        .map(|bar| kdj::OhlcvBar {
            trade_date: bar.trade_date,
            open: bar.open,
            high: bar.high,
            low: bar.low,
            close: bar.close,
            volume: bar.vol,
            amount: bar.amount,
        })
        .collect::<Vec<_>>();
    let ohlc_bars = bars
        .iter()
        .map(|bar| czsc::OhlcBar {
            trade_date: bar.trade_date,
            open: bar.open,
            high: bar.high,
            low: bar.low,
            close: bar.close,
        })
        .collect::<Vec<_>>();
    let rolling_bars = bars
        .iter()
        .map(|bar| rolling::RollingBar {
            trade_date: bar.trade_date,
            open: bar.open,
            high: bar.high,
            low: bar.low,
            close: bar.close,
            volume: bar.vol,
            turnover_rate: bar.turnover_rate,
        })
        .collect::<Vec<_>>();
    let left_peak_bars = bars
        .iter()
        .map(|bar| left_peak::LeftPeakBar {
            trade_date: bar.trade_date,
            open: bar.open,
            high: bar.high,
            low: bar.low,
            close: bar.close,
        })
        .collect::<Vec<_>>();
    let macd_values = macd::calculate_multi_period_macd(&macd_bars, calendar)?;
    let kdj_values = kdj::calculate_multi_period_kdj(&ohlcv_bars, calendar, args.kdj_window)?;
    let mut czsc_values = czsc::calculate_multi_period_czsc_for_dates(
        &ohlc_bars,
        calendar,
        args.output_from,
        args.output_to,
    )?
    .into_iter();
    let mut rolling_values =
        rolling::calculate_rolling_for_dates(&rolling_bars, args.output_from, args.output_to)?
            .into_iter();
    let mut left_peak_values = left_peak::calculate_left_peak_for_dates(
        &left_peak_bars,
        args.output_from,
        args.output_to,
    )?
    .into_iter();
    let mut period_kdj_values: BTreeMap<(String, NaiveDate), kdj::KdjValue> = BTreeMap::new();
    for index in 0..bars.len() {
        let bar = &bars[index];
        let macd_row = &macd_values.rows[index];
        let kdj_row = &kdj_values[index];
        if includes_output(args, bar.trade_date) {
            let czsc_row = czsc_values
                .next()
                .context("missing CZSC value for output date")?;
            let left_peak_row = left_peak_values
                .next()
                .context("missing left peak value for output date")?;
            let rolling_row = rolling_values
                .next()
                .context("missing rolling value for output date")?;
            emit(
                args,
                output,
                "calc_daily_indicators",
                indicator_row(dataset, ts_code, bar.trade_date, args, macd_row, kdj_row),
            )?;
            emit(
                args,
                output,
                "calc_daily_czsc_structure",
                czsc_row_value(dataset, ts_code, bar.trade_date, args, &czsc_row),
            )?;
            emit(
                args,
                output,
                "calc_daily_rolling_factors",
                rolling_row_value(dataset, ts_code, args, &rolling_row),
            )?;
            emit(
                args,
                output,
                "calc_daily_left_peak",
                left_peak_row_value(dataset, ts_code, args, &left_peak_row),
            )?;
            if let Some(daily) = macd_row.daily {
                emit(
                    args,
                    output,
                    "calc_indicator_state",
                    state_row(StateRowInput {
                        dataset,
                        ts_code,
                        period_type: "daily",
                        start: bar.trade_date,
                        end: bar.trade_date,
                        calc_version: &args.calc_version,
                        macd: Some(daily),
                        kdj: kdj_row.daily,
                    }),
                )?;
            }
        }
        if period_is_complete_for_input(calendar, bars, index, "weekly") {
            if let Some(value) = kdj_row.weekly {
                if let Some(end) = period_end(calendar, bar.trade_date, "weekly") {
                    period_kdj_values.insert(("weekly".to_string(), end), value);
                }
            }
        }
        if period_is_complete_for_input(calendar, bars, index, "monthly") {
            if let Some(value) = kdj_row.monthly {
                if let Some(end) = period_end(calendar, bar.trade_date, "monthly") {
                    period_kdj_values.insert(("monthly".to_string(), end), value);
                }
            }
        }
    }
    for period in &macd_values.completed_periods {
        if !includes_output(args, period.period_end_date) {
            continue;
        }
        let period_type = match period.period_type {
            macd::PeriodType::Weekly => "weekly",
            macd::PeriodType::Monthly => "monthly",
        };
        emit(
            args,
            output,
            "calc_indicator_state",
            state_row(StateRowInput {
                dataset,
                ts_code,
                period_type,
                start: period.period_start_date,
                end: period.period_end_date,
                calc_version: &args.calc_version,
                macd: Some(period.state),
                kdj: period_kdj_values
                    .get(&(period_type.to_string(), period.period_end_date))
                    .copied(),
            }),
        )?;
    }
    for row in period_bar_rows(dataset, ts_code, args, calendar, bars) {
        emit(args, output, "calc_period_bars", row)?;
    }
    Ok(())
}

fn includes_output(args: &Args, date: NaiveDate) -> bool {
    !args.output_from.is_some_and(|from| date < from) && !args.output_to.is_some_and(|to| date > to)
}

fn emit<W: Write>(args: &Args, output: &mut W, table: &'static str, row: Value) -> Result<()> {
    let date = row
        .get("trade_date")
        .or_else(|| row.get("period_end_date"))
        .and_then(Value::as_str)
        .map(parse_date)
        .transpose()?;
    if date.is_some_and(|date| !includes_output(args, date)) {
        return Ok(());
    }
    serde_json::to_writer(&mut *output, &OutputRow { table, row })?;
    output.write_all(b"\n")?;
    Ok(())
}

fn indicator_row(
    dataset: &str,
    ts_code: &str,
    date: NaiveDate,
    args: &Args,
    macd: &macd::MacdAsofRow,
    kdj: &kdj::KdjAsofRow,
) -> Value {
    let mut row = json!({"source_dataset": dataset, "ts_code": ts_code, "trade_date": date, "calc_version": args.calc_version});
    set_macd(&mut row, "daily", macd.daily, macd.daily_dea_percentile);
    set_macd(&mut row, "weekly", macd.weekly, macd.weekly_dea_percentile);
    set_macd(
        &mut row,
        "monthly",
        macd.monthly,
        macd.monthly_dea_percentile,
    );
    set_kdj(&mut row, "weekly", kdj.weekly);
    set_kdj(&mut row, "monthly", kdj.monthly);
    row
}

fn set_macd(
    row: &mut Value,
    prefix: &str,
    value: Option<macd::MacdValue>,
    percentile: Option<f64>,
) {
    let object = row.as_object_mut().expect("object row");
    let names = [
        ("dif_asof", value.map(|v| v.dif)),
        ("dea_asof", value.map(|v| v.dea)),
        ("hist_x2_asof", value.map(|v| v.hist_x2)),
        ("dea_pctile_asof", percentile),
        ("period_count", value.map(|v| v.period_count as f64)),
    ];
    for (suffix, item) in names {
        object.insert(
            format!("{prefix}_macd_{suffix}"),
            item.map_or(Value::Null, |v| json!(v)),
        );
    }
}

fn set_kdj(row: &mut Value, prefix: &str, value: Option<kdj::KdjValue>) {
    let object = row.as_object_mut().expect("object row");
    object.insert(
        format!("{prefix}_kdj_k_asof"),
        value.map_or(Value::Null, |v| json!(v.k)),
    );
    object.insert(
        format!("{prefix}_kdj_d_asof"),
        value.map_or(Value::Null, |v| json!(v.d)),
    );
    object.insert(
        format!("{prefix}_kdj_j_asof"),
        value.map_or(Value::Null, |v| json!(v.j)),
    );
}

struct StateRowInput<'a> {
    dataset: &'a str,
    ts_code: &'a str,
    period_type: &'a str,
    start: NaiveDate,
    end: NaiveDate,
    calc_version: &'a str,
    macd: Option<macd::MacdValue>,
    kdj: Option<kdj::KdjValue>,
}

fn state_row(row: StateRowInput<'_>) -> Value {
    json!({
        "source_dataset": row.dataset,
        "ts_code": row.ts_code,
        "period_type": row.period_type,
        "period_start_date": row.start,
        "period_end_date": row.end,
        "macd_ema12": row.macd.map(|value| value.ema12),
        "macd_ema26": row.macd.map(|value| value.ema26),
        "macd_dea": row.macd.map(|value| value.dea),
        "macd_period_count": row.macd.map(|value| value.period_count),
        "kdj_k": row.kdj.map(|value| value.k),
        "kdj_d": row.kdj.map(|value| value.d),
        "kdj_period_count": row.kdj.map(|value| value.period_count),
        "calc_version": row.calc_version,
    })
}

fn period_end(calendar: &[NaiveDate], date: NaiveDate, period_type: &str) -> Option<NaiveDate> {
    let key = period_key(period_type, date);
    let position = calendar.partition_point(|candidate| *candidate <= date);
    calendar[position..]
        .iter()
        .take_while(|candidate| period_key(period_type, **candidate) == key)
        .last()
        .copied()
        .or_else(|| {
            calendar[..position]
                .last()
                .copied()
                .filter(|candidate| period_key(period_type, *candidate) == key)
        })
}

fn period_is_complete_for_input(
    calendar: &[NaiveDate],
    bars: &[Bar],
    index: usize,
    period_type: &str,
) -> bool {
    let date = bars[index].trade_date;
    let next_period = bars
        .get(index + 1)
        .map(|next| period_key(period_type, next.trade_date));
    let current_period = period_key(period_type, date);
    next_period
        .as_ref()
        .is_some_and(|next| *next != current_period)
        || period_end(calendar, date, period_type).is_some_and(|end| end == date)
}

fn rolling_row_value(
    dataset: &str,
    ts_code: &str,
    args: &Args,
    value: &rolling::RollingValue,
) -> Value {
    json!({
        "source_dataset": dataset,
        "ts_code": ts_code,
        "trade_date": value.trade_date,
        "ma25_qfq": value.ma25_qfq,
        "ma144_qfq": value.ma144_qfq,
        "ma220_qfq": value.ma220_qfq,
        "high_20_qfq": value.high_20_qfq,
        "high_90_qfq": value.high_90_qfq,
        "low_90_qfq": value.low_90_qfq,
        "high_120_qfq": value.high_120_qfq,
        "low_120_qfq": value.low_120_qfq,
        "close_to_20d_max_close_pct": value.close_to_20d_max_close_pct,
        "position_90d": value.position_90d,
        "position_120d": value.position_120d,
        "volume_ma5": value.volume_ma5,
        "volume_ma20": value.volume_ma20,
        "volume_to_ma5_ratio": value.volume_to_ma5_ratio,
        "volume_to_ma20_ratio": value.volume_to_ma20_ratio,
        "volume_ma5_to_ma20_ratio": value.volume_ma5_to_ma20_ratio,
        "turnover_rate_ma5": value.turnover_rate_ma5,
        "turnover_to_ma5_ratio": value.turnover_to_ma5_ratio,
        "range_compression_20d": value.range_compression_20d,
        "range_compression_40d": value.range_compression_40d,
        "calc_version": args.calc_version,
    })
}

fn left_peak_row_value(
    dataset: &str,
    ts_code: &str,
    args: &Args,
    value: &left_peak::LeftPeakValue,
) -> Value {
    json!({
        "source_dataset": dataset,
        "ts_code": ts_code,
        "trade_date": value.trade_date,
        "left_peak_date": value.left_peak_date,
        "left_peak_high": value.left_peak_high,
        "breakout_date": value.breakout_date,
        "breakout_close": value.breakout_close,
        "breakout_body_above_left_peak": value.breakout_body_above_left_peak,
        "first_bear_date": value.first_bear_date,
        "first_bear_open": value.first_bear_open,
        "first_bear_missing": value.first_bear_missing,
        "pick_close": value.pick_close,
        "b_div_a": value.b_div_a,
        "abs_ba_minus_1": value.abs_ba_minus_1,
        "a_lt_b": value.a_lt_b,
        "is_valid": value.is_valid,
        "status": value.status,
        "calc_version": args.calc_version,
    })
}

fn czsc_row_value(
    dataset: &str,
    ts_code: &str,
    date: NaiveDate,
    args: &Args,
    value: &czsc::MultiPeriodCzscRow,
) -> Value {
    let mut row = json!({"source_dataset": dataset, "ts_code": ts_code, "trade_date": date, "calc_version": args.calc_version, "daily_period_complete": value.daily.period_complete, "weekly_period_complete": value.weekly.period_complete, "monthly_period_complete": value.monthly.period_complete});
    set_czsc(&mut row, "daily", &value.daily);
    set_czsc(&mut row, "weekly", &value.weekly);
    set_czsc(&mut row, "monthly", &value.monthly);
    row
}

fn set_czsc(row: &mut Value, prefix: &str, value: &czsc::CzscSnapshot) {
    let object = row.as_object_mut().expect("object row");
    let fields = json!(value);
    let source = fields.as_object().expect("snapshot object");
    for (name, item) in source {
        if name == "trade_date" || name == "period_complete" {
            continue;
        }
        object.insert(format!("{prefix}_{name}"), item.clone());
    }
}

fn period_bar_rows(
    dataset: &str,
    ts_code: &str,
    args: &Args,
    calendar: &[NaiveDate],
    bars: &[Bar],
) -> Vec<Value> {
    let mut grouped: BTreeMap<(String, String), Vec<&Bar>> = BTreeMap::new();
    let first_relevant = args.output_from.map(|from| {
        let week_start = from
            .checked_sub_signed(chrono::Duration::days(i64::from(
                from.weekday().num_days_from_monday(),
            )))
            .unwrap_or(NaiveDate::MIN);
        let month_start = from.with_day(1).expect("first day of a month exists");
        week_start.min(month_start)
    });
    for bar in bars {
        if first_relevant.is_some_and(|first| bar.trade_date < first) {
            continue;
        }
        if args.output_to.is_some_and(|last| bar.trade_date > last) {
            break;
        }
        for period_type in ["weekly", "monthly"] {
            grouped
                .entry((
                    period_type.to_string(),
                    period_key(period_type, bar.trade_date),
                ))
                .or_default()
                .push(bar);
        }
    }
    grouped.into_iter().filter_map(|((period_type, _key), values)| {
        let last_date = values.last()?.trade_date;
        let period_end = period_end(calendar, last_date, period_type.as_str())?;
        if period_end > last_date || !includes_output(args, period_end) { return None; }
        let first = values.first()?;
        let period_start_date = if period_type == "weekly" {
            first.trade_date - chrono::Duration::days(i64::from(first.trade_date.weekday().num_days_from_monday()))
        } else {
            NaiveDate::from_ymd_opt(first.trade_date.year(), first.trade_date.month(), 1)?
        };
        let open = first.open;
        let high = values.iter().filter_map(|bar| bar.high).reduce(f64::max);
        let low = values.iter().filter_map(|bar| bar.low).reduce(f64::min);
        let close = values.iter().rev().find_map(|bar| bar.close);
        let vol = values.iter().filter_map(|bar| bar.vol).reduce(|a,b| a+b);
        let amount = values.iter().filter_map(|bar| bar.amount).reduce(|a,b| a+b);
        Some(json!({"source_dataset": dataset, "ts_code": ts_code, "period_type": period_type, "period_start_date": period_start_date, "period_end_date": period_end, "price_basis": args.price_basis, "open": open, "high": high, "low": low, "close": close, "vol": vol, "amount": amount, "calc_version": args.calc_version}))
    }).collect()
}

fn period_key(period_type: &str, date: NaiveDate) -> String {
    if period_type == "weekly" {
        let iso = date.iso_week();
        format!("{}-W{:02}", iso.year(), iso.week())
    } else {
        format!("{}-{:02}", date.year(), date.month())
    }
}

fn parse_number(value: Option<&Value>) -> Result<Option<f64>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(value)) => value
            .as_f64()
            .map(Some)
            .ok_or_else(|| anyhow!("number is not representable as f64")),
        Some(Value::String(value)) if value.trim().is_empty() => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.trim().parse()?)),
        Some(other) => bail!("expected numeric value or null, got {other}"),
    }
}

/// Calculate a bounded instrument batch from cached history, emitting only the target slice.
pub(crate) fn run_cached(
    input: &Path,
    calendar: &Path,
    database: &Path,
    output: &Path,
    from: NaiveDate,
    to: NaiveDate,
    price_basis: &str,
) -> Result<()> {
    run(Args {
        input: input.to_string_lossy().into_owned(),
        calendar: calendar.to_path_buf(),
        source_dataset: None,
        changed_from: None,
        calc_version: "v3-macd-kdj-czsc-1".into(),
        price_basis: price_basis.into(),
        kdj_window: 9,
        output: output.to_string_lossy().into_owned(),
        database: Some(database.to_path_buf()),
        write: true,
        scope: vec![],
        output_from: Some(from),
        output_to: Some(to),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Weekday};

    #[test]
    fn period_end_matches_calendar_search_across_gaps_and_year_boundaries() {
        let start = NaiveDate::from_ymd_opt(2024, 12, 1).unwrap();
        let calendar: Vec<_> = (0..120)
            .map(|offset| start + Duration::days(offset))
            .filter(|date| !matches!(date.weekday(), Weekday::Sat | Weekday::Sun))
            .filter(|date| *date != NaiveDate::from_ymd_opt(2025, 1, 1).unwrap())
            .collect();
        for offset in -10..130 {
            let date = start + Duration::days(offset);
            for period_type in ["weekly", "monthly"] {
                let expected = calendar
                    .iter()
                    .rev()
                    .find(|candidate| {
                        period_key(period_type, **candidate) == period_key(period_type, date)
                    })
                    .copied();
                assert_eq!(
                    period_end(&calendar, date, period_type),
                    expected,
                    "{period_type} {date}"
                );
            }
        }
    }

    #[test]
    fn bounded_output_matches_full_history_filtered_by_date() {
        let start = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        let calendar: Vec<_> = (0..120)
            .map(|offset| start + Duration::days(offset))
            .filter(|date| !matches!(date.weekday(), Weekday::Sat | Weekday::Sun))
            .collect();
        let bars: Vec<_> = calendar
            .iter()
            .enumerate()
            .map(|(index, &trade_date)| {
                let close = 100.0 + (index as f64 / 4.0).sin() * 8.0;
                Bar {
                    source_dataset: "stock_stk_factor_pro".into(),
                    ts_code: "000001.SZ".into(),
                    trade_date,
                    open: Some(close - 0.3),
                    high: Some(close + 1.0),
                    low: Some(close - 1.0),
                    close: Some(close),
                    vol: Some(1000.0),
                    amount: Some(10000.0),
                    turnover_rate: Some(1.0),
                }
            })
            .collect();
        let mut args = Args {
            input: String::new(),
            calendar: PathBuf::new(),
            source_dataset: None,
            changed_from: None,
            calc_version: "test".into(),
            price_basis: "qfq".into(),
            kdj_window: 9,
            output: String::new(),
            database: None,
            write: false,
            scope: vec![],
            output_from: None,
            output_to: None,
        };
        let mut full = Vec::new();
        calculate_group(&args, &calendar, &bars, &mut full).unwrap();
        let parse_lines = |bytes: &[u8]| -> Vec<Value> {
            bytes
                .split(|byte| *byte == b'\n')
                .filter(|line| !line.is_empty())
                .map(|line| serde_json::from_slice(line).unwrap())
                .collect()
        };
        for (from, to) in [
            (Some(calendar[45]), Some(calendar[69])),
            (Some(calendar[20]), Some(calendar[23])),
            (None, Some(calendar[23])),
            (Some(calendar[65]), None),
            (Some(calendar[65]), Some(calendar[23])),
        ] {
            args.output_from = from;
            args.output_to = to;
            let mut bounded = Vec::new();
            calculate_group(&args, &calendar, &bars, &mut bounded).unwrap();
            let expected: Vec<_> = parse_lines(&full)
                .into_iter()
                .filter(|envelope| {
                    let row = &envelope["row"];
                    let date = row
                        .get("trade_date")
                        .or_else(|| row.get("period_end_date"))
                        .and_then(Value::as_str)
                        .map(parse_date)
                        .transpose()
                        .unwrap();
                    date.is_none_or(|date| includes_output(&args, date))
                })
                .collect();
            assert_eq!(parse_lines(&bounded), expected);
        }
    }
}
