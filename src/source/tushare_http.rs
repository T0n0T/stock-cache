//! Direct asynchronous HTTP transport for the Tushare JSON API.
//!
//! The transport is intentionally API-agnostic: callers provide the endpoint
//! name, selected fields and parameter map, while this module owns connection
//! reuse, pagination, response validation and transient retry handling.

use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Context, Result};
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use serde_json::{json, Value};

const DEFAULT_ENDPOINT: &str = "https://api.tushare.pro";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_PAGE_SIZE: usize = 5_000;
const DEFAULT_MAX_PAGES: usize = 1_000;
const DEFAULT_MAX_RETRIES: usize = 3;
const DEFAULT_RETRY_BACKOFF: Duration = Duration::from_millis(200);
// Tushare documents a 500 requests/minute limit for this token. Keep a small
// margin because their accounting window is opaque and retries count too.
const MAX_REQUESTS_PER_MINUTE: usize = 500;
const DEFAULT_REQUESTS_PER_MINUTE: usize = 450;

/// Configuration for [`TushareHttpClient`].
#[derive(Debug, Clone)]
pub struct TushareHttpConfig {
    pub endpoint: String,
    pub token: String,
    pub timeout: Duration,
    pub page_size: usize,
    pub max_pages: usize,
    /// Number of retries after the initial request.
    pub max_retries: usize,
    pub retry_backoff: Duration,
    pub requests_per_minute: usize,
}

impl TushareHttpConfig {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            endpoint: DEFAULT_ENDPOINT.to_string(),
            token: token.into(),
            timeout: DEFAULT_TIMEOUT,
            page_size: DEFAULT_PAGE_SIZE,
            max_pages: DEFAULT_MAX_PAGES,
            max_retries: DEFAULT_MAX_RETRIES,
            retry_backoff: DEFAULT_RETRY_BACKOFF,
            requests_per_minute: DEFAULT_REQUESTS_PER_MINUTE,
        }
    }

    /// Build configuration from environment variables.
    ///
    /// `TUSHARE_TOKEN` is required. `TUSHARE_ENDPOINT` is optional and is
    /// useful for a local proxy or a fixture server. Numeric overrides are
    /// intentionally strict so an invalid deployment cannot silently use a
    /// surprising request policy.
    pub fn from_env() -> Result<Self> {
        let token = std::env::var("TUSHARE_TOKEN")
            .context("TUSHARE_TOKEN is required for the Rust Tushare HTTP transport")?;
        let mut config = Self::new(token);
        if let Ok(endpoint) = std::env::var("TUSHARE_ENDPOINT") {
            if !endpoint.trim().is_empty() {
                config.endpoint = endpoint;
            }
        }
        if let Some(value) = env_usize("TUSHARE_HTTP_PAGE_SIZE")? {
            config.page_size = value;
        }
        if let Some(value) = env_usize("TUSHARE_HTTP_MAX_PAGES")? {
            config.max_pages = value;
        }
        if let Some(value) = env_usize("TUSHARE_HTTP_MAX_RETRIES")? {
            config.max_retries = value;
        }
        if let Some(value) = env_u64("TUSHARE_HTTP_TIMEOUT_SECS")? {
            config.timeout = Duration::from_secs(value);
        }
        if let Some(value) = env_u64("TUSHARE_HTTP_RETRY_BACKOFF_MS")? {
            config.retry_backoff = Duration::from_millis(value);
        }
        if let Some(value) = env_usize("TUSHARE_HTTP_REQUESTS_PER_MINUTE")? {
            config.requests_per_minute = value;
        }
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        if self.endpoint.trim().is_empty() {
            bail!("Tushare HTTP endpoint must not be empty");
        }
        if self.token.trim().is_empty() {
            bail!("Tushare token must not be empty");
        }
        if self.timeout.is_zero() {
            bail!("Tushare HTTP timeout must be positive");
        }
        if self.page_size == 0 {
            bail!("Tushare HTTP page_size must be positive");
        }
        if self.max_pages == 0 {
            bail!("Tushare HTTP max_pages must be positive");
        }
        if self.requests_per_minute == 0 {
            bail!("Tushare HTTP requests_per_minute must be positive");
        }
        if self.requests_per_minute > MAX_REQUESTS_PER_MINUTE {
            bail!("Tushare HTTP requests_per_minute must not exceed {MAX_REQUESTS_PER_MINUTE}");
        }
        Ok(())
    }
}

impl Default for TushareHttpConfig {
    fn default() -> Self {
        Self::new(String::new())
    }
}

/// A reusable reqwest client backed by one connection pool.
#[derive(Clone)]
pub struct TushareHttpClient {
    client: Client,
    config: TushareHttpConfig,
    limiter: Arc<RequestLimiter>,
}

struct RequestLimiter {
    limit: usize,
    window: Duration,
    requests: Mutex<VecDeque<Instant>>,
}

impl RequestLimiter {
    fn new(limit: usize) -> Self {
        Self {
            limit,
            window: Duration::from_secs(60),
            requests: Mutex::new(VecDeque::with_capacity(limit)),
        }
    }

    async fn acquire(&self) {
        loop {
            let wait = {
                let now = Instant::now();
                let mut requests = self.requests.lock().expect("request limiter poisoned");
                while requests
                    .front()
                    .is_some_and(|started| now.duration_since(*started) >= self.window)
                {
                    requests.pop_front();
                }
                if requests.len() < self.limit {
                    requests.push_back(now);
                    None
                } else {
                    Some(self.window - now.duration_since(*requests.front().unwrap()))
                }
            };
            if let Some(wait) = wait {
                tokio::time::sleep(wait).await;
            } else {
                return;
            }
        }
    }
}

impl std::fmt::Debug for TushareHttpClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TushareHttpClient")
            .field("endpoint", &self.config.endpoint)
            .field("timeout", &self.config.timeout)
            .field("page_size", &self.config.page_size)
            .field("max_pages", &self.config.max_pages)
            .field("max_retries", &self.config.max_retries)
            .field("requests_per_minute", &self.config.requests_per_minute)
            .finish_non_exhaustive()
    }
}

impl TushareHttpClient {
    pub fn new(config: TushareHttpConfig) -> Result<Self> {
        config.validate()?;
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .context("failed to build Tushare HTTP client")?;
        let limiter = Arc::new(RequestLimiter::new(config.requests_per_minute));
        Ok(Self {
            client,
            config,
            limiter,
        })
    }

    pub fn config(&self) -> &TushareHttpConfig {
        &self.config
    }

    /// Fetch a generated dataset using the same metadata contract as the
    /// compatibility Python worker.
    pub async fn fetch_dataset(
        &self,
        dataset: &crate::metadata::tushare_fields::DatasetMetadata,
        params: &BTreeMap<String, Value>,
    ) -> Result<Vec<BTreeMap<String, Value>>> {
        let fields = dataset
            .fields
            .iter()
            .map(|field| field.name)
            .collect::<Vec<_>>();
        self.fetch_api(dataset.api, &fields, params).await
    }

    /// Fetch all rows for one Tushare API, following `limit`/`offset` pages.
    pub async fn fetch_api(
        &self,
        api: &str,
        fields: &[&str],
        params: &BTreeMap<String, Value>,
    ) -> Result<Vec<BTreeMap<String, Value>>> {
        if api.trim().is_empty() {
            bail!("Tushare api_name must not be empty");
        }
        if fields.is_empty() {
            bail!("Tushare fields must not be empty");
        }

        let mut rows = Vec::new();
        let mut offset = 0usize;
        for page in 0..self.config.max_pages {
            let mut page_params = params.clone();
            page_params.insert("limit".to_string(), json!(self.config.page_size));
            page_params.insert("offset".to_string(), json!(offset));
            let request = json!({
                "api_name": api,
                "token": self.config.token,
                "params": page_params,
                "fields": fields,
            });
            let response = self.send_with_retry(&request).await?;
            let explicit_has_more = response.data.as_ref().and_then(|data| data.has_more);
            let page_rows = decode_response(response, fields)?;
            let item_count = page_rows.len();
            rows.extend(page_rows);

            let has_more =
                explicit_has_more.unwrap_or(item_count == self.config.page_size && item_count > 0);
            if !has_more {
                return Ok(rows);
            }
            if item_count == 0 {
                bail!("Tushare response reported has_more with an empty page");
            }
            offset = offset
                .checked_add(item_count)
                .ok_or_else(|| anyhow!("Tushare pagination offset overflow"))?;
            if page + 1 == self.config.max_pages {
                bail!(
                    "Tushare response exceeded max_pages ({})",
                    self.config.max_pages
                );
            }
        }
        unreachable!("max_pages loop returns or errors")
    }

    async fn send_with_retry(&self, request: &Value) -> Result<ApiResponse> {
        for attempt in 0..=self.config.max_retries {
            self.limiter.acquire().await;
            let result = self
                .client
                .post(&self.config.endpoint)
                .json(request)
                .send()
                .await;
            match result {
                Ok(response) => {
                    let status = response.status();
                    let body = response
                        .bytes()
                        .await
                        .context("failed to read Tushare HTTP response")?;
                    if status.is_success() {
                        return serde_json::from_slice(&body)
                            .context("invalid Tushare JSON response");
                    }
                    if !is_retryable_status(status) || attempt == self.config.max_retries {
                        return Err(http_error(status, &body));
                    }
                }
                Err(error) => {
                    if attempt == self.config.max_retries {
                        return Err(error).context("Tushare HTTP request failed");
                    }
                }
            }
            let multiplier = u32::try_from(attempt + 1).unwrap_or(u32::MAX);
            tokio::time::sleep(self.config.retry_backoff.saturating_mul(multiplier)).await;
        }
        unreachable!("retry loop returns")
    }
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    #[serde(default)]
    code: i64,
    #[serde(default)]
    msg: Option<String>,
    #[serde(default)]
    data: Option<ApiData>,
}

#[derive(Debug, Deserialize)]
struct ApiData {
    #[serde(default)]
    fields: Vec<String>,
    #[serde(default)]
    items: Vec<Vec<Value>>,
    #[serde(default)]
    has_more: Option<bool>,
}

fn decode_response(
    response: ApiResponse,
    requested_fields: &[&str],
) -> Result<Vec<BTreeMap<String, Value>>> {
    if response.code != 0 {
        let message = response
            .msg
            .unwrap_or_else(|| "unknown Tushare error".to_string());
        bail!("Tushare API error (code={}): {message}", response.code);
    }
    let Some(data) = response.data else {
        return Ok(Vec::new());
    };
    let fields = if data.fields.is_empty() {
        requested_fields
            .iter()
            .map(|field| (*field).to_string())
            .collect::<Vec<_>>()
    } else {
        data.fields
    };
    if fields.is_empty() && !data.items.is_empty() {
        bail!("Tushare response has rows but no fields");
    }
    let mut rows = Vec::with_capacity(data.items.len());
    for (index, item) in data.items.into_iter().enumerate() {
        if item.len() != fields.len() {
            bail!(
                "Tushare response row {index} has {} values but {} fields",
                item.len(),
                fields.len()
            );
        }
        rows.push(fields.iter().cloned().zip(item).collect());
    }
    Ok(rows)
}

fn http_error(status: StatusCode, body: &[u8]) -> anyhow::Error {
    let detail = serde_json::from_slice::<ApiResponse>(body)
        .ok()
        .and_then(|response| response.msg)
        .filter(|message| !message.trim().is_empty())
        .unwrap_or_else(|| format!("HTTP status {status}"));
    anyhow!("Tushare HTTP request failed ({status}): {detail}")
}

fn is_retryable_status(status: StatusCode) -> bool {
    status.is_server_error()
        || status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::REQUEST_TIMEOUT
}

fn env_usize(name: &str) -> Result<Option<usize>> {
    std::env::var(name)
        .ok()
        .map(|value| {
            value
                .parse::<usize>()
                .with_context(|| format!("invalid {name}={value:?}"))
        })
        .transpose()
}

fn env_u64(name: &str) -> Result<Option<u64>> {
    std::env::var(name)
        .ok()
        .map(|value| {
            value
                .parse::<u64>()
                .with_context(|| format!("invalid {name}={value:?}"))
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        sync::Mutex,
        task::JoinHandle,
    };

    async fn spawn_server(
        responses: Vec<(u16, String)>,
    ) -> (String, Arc<Mutex<Vec<String>>>, JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&requests);
        let task = tokio::spawn(async move {
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = read_request(&mut stream).await;
                captured.lock().await.push(request);
                let response = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(), body
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        (endpoint, requests, task)
    }

    async fn read_request(stream: &mut tokio::net::TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut marker = [0u8; 4];
        loop {
            let mut byte = [0u8; 1];
            stream.read_exact(&mut byte).await.unwrap();
            bytes.push(byte[0]);
            marker.rotate_left(1);
            marker[3] = byte[0];
            if marker == *b"\r\n\r\n" {
                break;
            }
        }
        let headers = String::from_utf8_lossy(&bytes).to_string();
        let length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then_some(value.trim())
            })
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        let mut body = vec![0; length];
        stream.read_exact(&mut body).await.unwrap();
        format!("{headers}{}", String::from_utf8_lossy(&body))
    }

    fn config(endpoint: String) -> TushareHttpConfig {
        TushareHttpConfig {
            endpoint,
            token: "secret-token".to_string(),
            timeout: Duration::from_secs(2),
            page_size: 2,
            max_pages: 4,
            max_retries: 2,
            retry_backoff: Duration::from_millis(1),
            requests_per_minute: 450,
        }
    }

    #[tokio::test]
    async fn decodes_rows_and_uses_configured_endpoint() {
        let body = r#"{"code":0,"data":{"fields":["ts_code","trade_date"],"items":[["000001.SZ","20260625"]],"has_more":false}}"#;
        let (endpoint, requests, task) = spawn_server(vec![(200, body.to_string())]).await;
        let client = TushareHttpClient::new(config(endpoint)).unwrap();
        let rows = client
            .fetch_api("daily", &["ts_code", "trade_date"], &BTreeMap::new())
            .await
            .unwrap();
        task.await.unwrap();
        assert_eq!(rows[0]["ts_code"], "000001.SZ");
        assert!(requests.lock().await[0].contains("secret-token"));
        assert!(requests.lock().await[0].contains("\"api_name\":\"daily\""));
    }

    #[tokio::test]
    async fn follows_pagination() {
        let first = r#"{"code":0,"data":{"fields":["id"],"items":[[1],[2]],"has_more":true}}"#;
        let second = r#"{"code":0,"data":{"fields":["id"],"items":[[3]],"has_more":false}}"#;
        let (endpoint, requests, task) =
            spawn_server(vec![(200, first.to_string()), (200, second.to_string())]).await;
        let client = TushareHttpClient::new(config(endpoint)).unwrap();
        let rows = client
            .fetch_api("demo", &["id"], &BTreeMap::new())
            .await
            .unwrap();
        task.await.unwrap();
        assert_eq!(rows.len(), 3);
        let captured = requests.lock().await;
        assert!(captured[0].contains("\"offset\":0"));
        assert!(captured[1].contains("\"offset\":2"));
    }

    #[tokio::test]
    async fn rejects_mismatched_field_count() {
        let body = r#"{"code":0,"data":{"fields":["id","name"],"items":[[1]],"has_more":false}}"#;
        let (endpoint, _, task) = spawn_server(vec![(200, body.to_string())]).await;
        let client = TushareHttpClient::new(config(endpoint)).unwrap();
        let error = client
            .fetch_api("demo", &["id", "name"], &BTreeMap::new())
            .await
            .unwrap_err();
        task.await.unwrap();
        assert!(error.to_string().contains("values but 2 fields"));
    }

    #[tokio::test]
    async fn does_not_retry_business_errors() {
        let body = r#"{"code":-2001,"msg":"permission denied","data":null}"#;
        let (endpoint, requests, task) = spawn_server(vec![(200, body.to_string())]).await;
        let client = TushareHttpClient::new(config(endpoint)).unwrap();
        let error = client
            .fetch_api("demo", &["id"], &BTreeMap::new())
            .await
            .unwrap_err();
        task.await.unwrap();
        assert!(error.to_string().contains("permission denied"));
        assert_eq!(requests.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn retries_transient_status() {
        let body = r#"{"code":0,"data":{"fields":["id"],"items":[[1]],"has_more":false}}"#;
        let (endpoint, requests, task) =
            spawn_server(vec![(503, "busy".to_string()), (200, body.to_string())]).await;
        let client = TushareHttpClient::new(config(endpoint)).unwrap();
        let rows = client
            .fetch_api("demo", &["id"], &BTreeMap::new())
            .await
            .unwrap();
        task.await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(requests.lock().await.len(), 2);
    }

    #[test]
    fn rejects_request_budget_above_provider_limit() {
        let mut config = config("http://127.0.0.1:1".to_string());
        config.requests_per_minute = 501;
        let error = TushareHttpClient::new(config).unwrap_err();
        assert!(error.to_string().contains("must not exceed 500"));
    }
}
