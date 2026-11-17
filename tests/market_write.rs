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
                } else {
                    let day = request["params"]["trade_date"]
                        .as_str()
                        .or_else(|| request["params"]["start_date"].as_str())
                        .unwrap();
                    let index = OPEN.iter().position(|d| *d == day).unwrap();
                    let close = 10.0 + index as f64;
                    let code = match api {
                        "stk_factor_pro" => "000001.SZ",
                        "idx_factor_pro" => request["params"]["ts_code"].as_str().unwrap(),
                        "ths_daily" => "885001.TI",
                        other => panic!("unexpected api {other}"),
                    };
                    if missing.load(Ordering::Relaxed) && api == "stk_factor_pro" {
                        vec![]
                    } else {
                        vec![fields
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
                            .collect()]
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
    let summary: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(summary["target_trade_dates"], json!(["2026-11-09"]));
    assert_eq!(summary["calculated_rows"], 15); // 4 daily tables + daily state, 3 instruments
    assert_eq!(server.requests.lock().unwrap().len(), 13); // calendar + 4 sessions * 3 sources
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
    assert_eq!(server.requests.lock().unwrap().len(), 17); // only calendar + new targets
    assert_eq!(
        scalar(
            database,
            "SELECT count(*)::DOUBLE FROM calc_daily_indicators"
        ),
        6.0
    );
    let third = invoke("calc", "2026-11-09");
    assert!(
        third.status.success(),
        "{}",
        String::from_utf8_lossy(&third.stderr)
    );
    assert_eq!(server.requests.lock().unwrap().len(), 17); // no HTTP in calc mode
    assert_eq!(
        scalar(
            database,
            "SELECT count(*)::DOUBLE FROM calc_daily_indicators"
        ),
        6.0
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
        6.0
    );
    let offline = invoke("calc", "2026-11-11");
    assert!(
        offline.status.success(),
        "{}",
        String::from_utf8_lossy(&offline.stderr)
    );
    assert_eq!(server.requests.lock().unwrap().len(), 21);
    assert_eq!(
        scalar(
            database,
            "SELECT count(*)::DOUBLE FROM calc_daily_indicators"
        ),
        9.0
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
    assert_eq!(server.requests.lock().unwrap().len(), 21);
    let repair = invoke("full", "2026-11-11");
    assert!(
        repair.status.success(),
        "{}",
        String::from_utf8_lossy(&repair.stderr)
    );
    assert_eq!(server.requests.lock().unwrap().len(), 26); // calendar, missing warmup, three targets
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
