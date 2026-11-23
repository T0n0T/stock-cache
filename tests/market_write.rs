use assert_cmd::Command;
use duckdb_neo::Parameters;
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    process::Output,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
};
use stock_cache::storage::duckdb;
use tempfile::tempdir;

const OPEN: [&str; 10] = [
    "20261104", "20261105", "20261106", "20261109", "20261110", "20261111", "20261112", "20261113",
    "20261116", "20261130",
];
struct Server {
    endpoint: String,
    stop: Arc<AtomicBool>,
    empty: Arc<AtomicBool>,
    requests: Arc<Mutex<Vec<Value>>>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let empty = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let (stopped, missing, recorded) = (stop.clone(), empty.clone(), requests.clone());
        let worker = thread::spawn(move || {
            for stream in listener.incoming() {
                if stopped.load(Ordering::Relaxed) {
                    break;
                }
                let mut stream = stream.unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                    .unwrap();
                let mut reader = BufReader::new(&mut stream);
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some((key, value)) = line.split_once(':') {
                        if key.eq_ignore_ascii_case("content-length") {
                            length = value.trim().parse().unwrap();
                        }
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let request: Value = serde_json::from_slice(&body).unwrap();
                recorded.lock().unwrap().push(request.clone());
                let fields = request["fields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap())
                    .collect::<Vec<_>>();
                let api = request["api_name"].as_str().unwrap();
                let items = if api == "trade_cal" {
                    OPEN.iter()
                        .map(|day| {
                            fields
                                .iter()
                                .map(|f| match *f {
                                    "cal_date" => json!(day),
                                    "is_open" => json!(1),
                                    _ => Value::Null,
                                })
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>()
                } else if api == "stock_basic" {
                    vec![fields
                        .iter()
                        .map(|f| match *f {
                            "ts_code" => json!("000001.SZ"),
                            "name" => json!("平安银行"),
                            _ => Value::Null,
                        })
                        .collect()]
                } else if api == "ths_index" {
                    vec![fields
                        .iter()
                        .map(|f| match *f {
                            "ts_code" => json!("885001.TI"),
                            "name" => json!("测试指数"),
                            _ => Value::Null,
                        })
                        .collect()]
                } else if api == "ths_member" {
                    vec![fields
                        .iter()
                        .map(|f| match *f {
                            "ts_code" => json!("885001.TI"),
                            "con_code" => json!("000001.SZ"),
                            _ => Value::Null,
                        })
                        .collect()]
                } else if api == "index_classify" {
                    [
                        ("801010.SI", "农林牧渔", "L1", "110000", "0"),
                        ("801016.SI", "种植业", "L2", "110100", "110000"),
                    ]
                    .into_iter()
                    .map(|(code, name, level, industry, parent)| {
                        fields
                            .iter()
                            .map(|f| match *f {
                                "index_code" => json!(code),
                                "industry_name" => json!(name),
                                "level" => json!(level),
                                "industry_code" => json!(industry),
                                "parent_code" => json!(parent),
                                "src" => json!("SW2021"),
                                "is_pub" => json!("1"),
                                _ => Value::Null,
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
                } else if api == "index_member_all" {
                    let current = request["params"]["is_new"].as_str() == Some("Y");
                    let rows = if current {
                        vec![
                            ("000001.SZ", "801016.SI", "种植业", Value::Null),
                            ("000002.SZ", "801016.SI", "种植业", Value::Null),
                        ]
                    } else {
                        vec![("000002.SZ", "801014.SI", "饲料", json!("20200102"))]
                    };
                    rows.into_iter()
                        .map(|(ts_code, l2_code, l2_name, out_date)| {
                            fields
                                .iter()
                                .map(|f| match *f {
                                    "l1_code" => json!("801010.SI"),
                                    "l1_name" => json!("农林牧渔"),
                                    "l2_code" => json!(l2_code),
                                    "l2_name" => json!(l2_name),
                                    "l3_code" => json!("850101.SI"),
                                    "l3_name" => json!("三级"),
                                    "ts_code" => json!(ts_code),
                                    "name" => json!("测试股票"),
                                    "in_date" => {
                                        json!(if current { "20200101" } else { "20190101" })
                                    }
                                    "out_date" => out_date.clone(),
                                    "is_new" => json!(if current { "Y" } else { "N" }),
                                    _ => Value::Null,
                                })
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>()
                } else if api == "moneyflow" {
                    let day = request["params"]["trade_date"].as_str().unwrap();
                    ["000001.SZ", "000002.SZ"]
                        .into_iter()
                        .map(|ts_code| {
                            fields
                                .iter()
                                .map(|f| match *f {
                                    "ts_code" => json!(ts_code),
                                    "trade_date" => json!(day),
                                    "buy_sm_amount" => json!(1.0),
                                    "sell_sm_amount" => json!(0.0),
                                    "buy_md_amount" => json!(2.0),
                                    "sell_md_amount" => json!(0.0),
                                    "buy_lg_amount" => json!(3.0),
                                    "sell_lg_amount" => json!(0.0),
                                    "buy_elg_amount" => json!(4.0),
                                    "sell_elg_amount" => json!(0.0),
                                    "net_mf_vol" => json!(5.0),
                                    "net_mf_amount" => json!(10.0),
                                    _ => Value::Null,
                                })
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>()
                } else if api == "index_weight" {
                    let index_code = request["params"]["index_code"].as_str().unwrap();
                    let start = request["params"]["start_date"].as_str().unwrap();
                    vec![fields
                        .iter()
                        .map(|f| match *f {
                            "index_code" => json!(index_code),
                            "trade_date" => json!(start),
                            "con_code" => json!("000001.SZ"),
                            "weight" => json!(100.0),
                            _ => Value::Null,
                        })
                        .collect()]
                } else {
                    let days = if let Some(day) = request["params"]["trade_date"].as_str() {
                        vec![day]
                    } else {
                        let start = request["params"]["start_date"].as_str().unwrap();
                        let end = request["params"]["end_date"].as_str().unwrap();
                        OPEN.iter()
                            .filter(|day| **day >= start && **day <= end)
                            .copied()
                            .collect::<Vec<_>>()
                    };
                    let code = match api {
                        "stk_factor_pro" => "000001.SZ",
                        "idx_factor_pro" => request["params"]["ts_code"].as_str().unwrap(),
                        "ths_daily" => "885001.TI",
                        other => panic!("unexpected api {other}"),
                    };
                    if missing.load(Ordering::Relaxed) && api == "stk_factor_pro" {
                        vec![]
                    } else {
                        days.into_iter()
                            .map(|day| {
                                let index = OPEN.iter().position(|d| *d == day).unwrap();
                                let close = 10.0 + index as f64;
                                fields
                                    .iter()
                                    .map(|f| match *f {
                                        "trade_date" => json!(day),
                                        "ts_code" => json!(code),
                                        "close_qfq" => json!(close),
                                        "open_qfq" => json!(close - 0.5),
                                        "high_qfq" => json!(close + 1.0),
                                        "low_qfq" => json!(close - 1.0),
                                        "close" => json!(1.0),
                                        "open" => json!(0.5),
                                        "high" => json!(2.0),
                                        "low" => json!(0.1),
                                        "vol" => json!(1000.0),
                                        "amount" => json!(10000.0),
                                        "turnover_rate" => json!(1.0),
                                        _ => Value::Null,
                                    })
                                    .collect()
                            })
                            .collect()
                    }
                };
                let response =
                    json!({"code":0,"msg":"", "data":{"fields":fields,"items":items}}).to_string();
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).unwrap();
            }
        });
        Self {
            endpoint,
            stop,
            empty,
            requests,
            thread: Some(worker),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(self.endpoint.strip_prefix("http://").unwrap());
        self.thread.take().unwrap().join().unwrap();
    }
}
fn scalar(database: &str, sql: &str) -> f64 {
    duckdb::with_connection(database, |connection| {
        let mut query = connection.query(sql, Parameters::None)?;
        let chunk = query.next_chunk()?.unwrap();
        let value = chunk
            .get_vector_at::<f64>(0)?
            .reader()?
            .get(0)?
            .copied()
            .unwrap();
        Ok(value)
    })
    .unwrap()
}
#[test]
fn full_workflow_uses_history_reuses_cache_and_recomputes_offline() {
    let server = Server::new();
    let root = tempdir().unwrap();
    let database = root.path().join("stock-cache.duckdb");
    let database = database.to_str().unwrap();
    let tables = root.path().join("tables.txt");
    fs::write(
        &tables,
        "stock_stk_factor_pro\nindex_idx_factor_pro\nindex_ths_daily\n",
    )
    .unwrap();
    let indexes = root.path().join("indexes.csv");
    fs::write(
        &indexes,
        "ts_code,name,category,enabled\n000001.SH,index,market,true\n",
    )
    .unwrap();
    let invoke = |mode: &str, day: &str| -> Output {
        Command::cargo_bin("stock-cache")
            .unwrap()
            .args([
                "write",
                "--mode",
                mode,
                "--start-date",
                day,
                "--end-date",
                day,
                "--database",
                database,
                "--max-concurrency",
                "2",
                "--chunk-size",
                "1",
            ])
            .env("TUSHARE_TOKEN", "fixture-token")
            .env("TUSHARE_ENDPOINT", &server.endpoint)
            .env("TUSHARE_HTTP_PAGE_SIZE", "5000")
            .env("TARGET_CACHE_TABLES_PATH", &tables)
            .env("INDEX_LIST_PATH", &indexes)
            .output()
            .unwrap()
    };
    let first = invoke("full", "2026-11-09");
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let progress = String::from_utf8_lossy(&first.stderr);
    assert!(
        progress.contains("[write] fetched 6/6 requests"),
        "{progress}"
    );
    assert!(
        progress.contains("[write] fetch complete; deriving industry moneyflow"),
        "{progress}"
    );
    assert!(
        progress.contains("[write] calculating stock_stk_factor_pro: 1/1 instruments"),
        "{progress}"
    );
    let summary = String::from_utf8_lossy(&first.stdout);
    assert!(
        summary.contains("Write completed successfully."),
        "{summary}"
    );
    assert!(summary.contains("Mode: full"), "{summary}");
    assert!(summary.contains("Target dates: 2026-11-09"), "{summary}");
    assert!(summary.contains("Calculated rows: 10"), "{summary}");
    assert!(!summary.trim_start().starts_with('{'), "{summary}");
    assert_eq!(server.requests.lock().unwrap().len(), 7); // calendar + stock history/target + one index range + target THS
    assert_eq!(
        scalar(
            database,
            "SELECT count(*)::DOUBLE FROM stock_stk_factor_pro"
        ),
        4.0
    );
    assert_eq!(scalar(database, "SELECT count(*)::DOUBLE FROM calc_daily_indicators WHERE trade_date < DATE '2026-11-09'"), 0.0);
    let mut ema12 = 10.0;
    let mut ema26 = 10.0;
    let mut dea = 0.0;
    for close in [11.0, 12.0, 13.0] {
        ema12 += (close - ema12) * 2.0 / 13.0;
        ema26 += (close - ema26) * 2.0 / 27.0;
        dea += ((ema12 - ema26) - dea) * 2.0 / 10.0;
    }
    let stored = scalar(database, "SELECT macd_ema12 FROM calc_indicator_state WHERE source_dataset='stock_stk_factor_pro' AND period_type='daily'");
    assert!((stored - ema12).abs() < 1e-12);
    let dif = scalar(database, "SELECT macd_ema26 FROM calc_indicator_state WHERE source_dataset='stock_stk_factor_pro' AND period_type='daily'");
    assert!((dif - ema26).abs() < 1e-12);
    assert!((scalar(database, "SELECT macd_dea FROM calc_indicator_state WHERE source_dataset='stock_stk_factor_pro' AND period_type='daily'")-dea).abs() < 1e-12);
    assert_eq!(scalar(database, "SELECT count(*)::DOUBLE FROM calc_indicator_state WHERE period_type IN ('weekly','monthly')"), 0.0);
    let second = invoke("full", "2026-11-10");
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(server.requests.lock().unwrap().len(), 11); // only calendar + new targets
    assert_eq!(
        scalar(
            database,
            "SELECT count(*)::DOUBLE FROM calc_daily_indicators"
        ),
        4.0
    );
    let third = invoke("calc", "2026-11-09");
    assert!(
        third.status.success(),
        "{}",
        String::from_utf8_lossy(&third.stderr)
    );
    assert_eq!(server.requests.lock().unwrap().len(), 11); // no HTTP in calc mode
    assert_eq!(
        scalar(
            database,
            "SELECT count(*)::DOUBLE FROM calc_daily_indicators"
        ),
        4.0
    );
    assert!((scalar(database, "SELECT macd_ema12 FROM calc_indicator_state WHERE source_dataset='stock_stk_factor_pro' AND period_type='daily' AND period_end_date=DATE '2026-11-09'")-ema12).abs() < 1e-12);
    let cache = invoke("cache", "2026-11-11");
    assert!(
        cache.status.success(),
        "{}",
        String::from_utf8_lossy(&cache.stderr)
    );
    assert_eq!(
        scalar(
            database,
            "SELECT count(*)::DOUBLE FROM calc_daily_indicators"
        ),
        4.0
    );
    let offline = invoke("calc", "2026-11-11");
    assert!(
        offline.status.success(),
        "{}",
        String::from_utf8_lossy(&offline.stderr)
    );
    assert_eq!(server.requests.lock().unwrap().len(), 15);
    assert_eq!(
        scalar(
            database,
            "SELECT count(*)::DOUBLE FROM calc_daily_indicators"
        ),
        6.0
    );
    duckdb::with_connection(database, |connection| {
        connection.execute(
            "DELETE FROM stock_stk_factor_pro WHERE trade_date=DATE '2026-11-04'",
            Parameters::None,
        )?;
        Ok(())
    })
    .unwrap();
    let incomplete = invoke("calc", "2026-11-11");
    assert!(!incomplete.status.success());
    assert!(
        String::from_utf8_lossy(&incomplete.stderr).contains("lacks historical source coverage")
    );
    assert_eq!(server.requests.lock().unwrap().len(), 15);
    let repair = invoke("full", "2026-11-11");
    assert!(
        repair.status.success(),
        "{}",
        String::from_utf8_lossy(&repair.stderr)
    );
    assert_eq!(server.requests.lock().unwrap().len(), 20); // calendar, missing stock warmup, stock/index/THS targets
    server.empty.store(true, Ordering::Relaxed);
    let missing = invoke("full", "2026-11-10");
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("no stock market data"));
    assert_eq!(
        scalar(
            database,
            "SELECT count(*)::DOUBLE FROM stock_stk_factor_pro"
        ),
        6.0
    );
}

#[test]
fn sw_l2_industry_moneyflow_is_derived_in_cache_mode() {
    let server = Server::new();
    let root = tempdir().unwrap();
    let database = root.path().join("sw.duckdb");
    let database = database.to_str().unwrap();
    let tables = root.path().join("tables.txt");
    fs::write(
        &tables,
        "stock_stk_factor_pro\nindex_idx_factor_pro\nindex_ths_daily\nstock_moneyflow\n",
    )
    .unwrap();
    let indexes = root.path().join("indexes.csv");
    fs::write(
        &indexes,
        "ts_code,name,category,enabled\n000001.SH,index,market,true\n801016.SI,种植业,industry,true\n",
    )
    .unwrap();
    let invoke = |args: &[&str]| -> Output {
        Command::cargo_bin("stock-cache")
            .unwrap()
            .args(args)
            .args(["--database", database])
            .env("TUSHARE_TOKEN", "fixture-token")
            .env("TUSHARE_ENDPOINT", &server.endpoint)
            .env("TUSHARE_HTTP_PAGE_SIZE", "5000")
            .env("TARGET_CACHE_TABLES_PATH", &tables)
            .env("INDEX_LIST_PATH", &indexes)
            .output()
            .unwrap()
    };
    let refresh = invoke(&["write", "--mode", "cache", "--refresh"]);
    assert!(
        refresh.status.success(),
        "{}",
        String::from_utf8_lossy(&refresh.stderr)
    );
    assert!(scalar(database, "SELECT count(*)::DOUBLE FROM index_weight") > 0.0);
    let run = invoke(&[
        "write",
        "--mode",
        "cache",
        "--start-date",
        "2026-11-09",
        "--end-date",
        "2026-11-09",
    ]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        scalar(database, "SELECT count(*)::DOUBLE FROM stock_moneyflow"),
        2.0
    );
    assert_eq!(
        scalar(
            database,
            "SELECT count(*)::DOUBLE FROM index_idx_factor_pro WHERE ts_code='801016.SI'"
        ),
        1.0
    );
    let filter = "WHERE industry_code='801016.SI' AND trade_date=DATE '2026-11-09'";
    assert_eq!(
        scalar(
            database,
            &format!("SELECT stock_count::DOUBLE FROM calc_daily_industry_moneyflow {filter}")
        ),
        2.0
    );
    assert_eq!(
        scalar(
            database,
            &format!("SELECT net_mf_amount FROM calc_daily_industry_moneyflow {filter}")
        ),
        20.0
    );
    assert_eq!(
        scalar(
            database,
            &format!("SELECT main_net_amount FROM calc_daily_industry_moneyflow {filter}")
        ),
        14.0
    );
}
