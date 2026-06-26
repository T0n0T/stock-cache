#![allow(dead_code)]

use std::{collections::BTreeMap, process::Stdio};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command},
    task::JoinHandle,
};

use crate::generated::tushare_fields::DatasetMetadata;

const DEFAULT_WORKER_PATH: &str = "runtime/tushare_worker.py";

#[derive(Debug, Serialize)]
struct WorkerRequest<'a> {
    id: &'a str,
    dataset: &'a str,
    api: &'a str,
    fields: Vec<&'a str>,
    params: &'a BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum WorkerFrame {
    #[serde(rename = "batch")]
    Batch { id: String, rows: Vec<Value> },
    #[serde(rename = "end")]
    End { id: String, row_count: usize },
    #[serde(rename = "error")]
    Error {
        id: String,
        message: String,
        retryable: bool,
    },
}

pub struct PythonWorker {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    stderr_task: JoinHandle<String>,
}

impl PythonWorker {
    pub fn command_from_env() -> Command {
        let mut command = Command::new("python3");
        let script = std::env::var("STOCK_CACHE_TUSHARE_WORKER")
            .unwrap_or_else(|_| DEFAULT_WORKER_PATH.to_string());
        command.arg(script);
        command
    }

    pub async fn spawn(mut command: Command) -> Result<Self> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to spawn Tushare worker")?;
        let stdin = child.stdin.take().context("worker stdin was not piped")?;
        let stdout = child.stdout.take().context("worker stdout was not piped")?;
        let stderr = child.stderr.take().context("worker stderr was not piped")?;
        let stderr_task = tokio::spawn(read_stderr(stderr));
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            stderr_task,
        })
    }

    pub async fn fetch_dataset(
        &mut self,
        id: &str,
        dataset: &DatasetMetadata,
        params: &BTreeMap<String, Value>,
    ) -> Result<Vec<BTreeMap<String, Value>>> {
        let request = WorkerRequest {
            id,
            dataset: dataset.id,
            api: dataset.api,
            fields: dataset.fields.iter().map(|field| field.name).collect(),
            params,
        };
        let mut line = serde_json::to_vec(&request)?;
        line.push(b'\n');
        self.stdin.write_all(&line).await?;
        self.stdin.flush().await?;

        let mut rows = Vec::new();
        loop {
            let mut frame_line = String::new();
            let bytes_read = self.stdout.read_line(&mut frame_line).await?;
            if bytes_read == 0 {
                return Err(anyhow!("Tushare worker exited before response ended"));
            }
            let frame: WorkerFrame = serde_json::from_str(frame_line.trim_end())
                .with_context(|| format!("invalid worker stdout frame: {frame_line:?}"))?;
            match frame {
                WorkerFrame::Batch {
                    id: frame_id,
                    rows: batch,
                } => {
                    ensure_frame_id(id, &frame_id)?;
                    for row in batch {
                        let object = row
                            .as_object()
                            .ok_or_else(|| anyhow!("worker row must be an object"))?;
                        rows.push(
                            object
                                .iter()
                                .map(|(key, value)| (key.clone(), value.clone()))
                                .collect(),
                        );
                    }
                }
                WorkerFrame::End {
                    id: frame_id,
                    row_count,
                } => {
                    ensure_frame_id(id, &frame_id)?;
                    if rows.len() != row_count {
                        return Err(anyhow!(
                            "worker row_count mismatch for {id}: expected {row_count}, got {}",
                            rows.len()
                        ));
                    }
                    return Ok(rows);
                }
                WorkerFrame::Error {
                    id: frame_id,
                    message,
                    retryable,
                } => {
                    ensure_frame_id(id, &frame_id)?;
                    return Err(anyhow!(
                        "worker error for {id} (retryable={retryable}): {message}"
                    ));
                }
            }
        }
    }
}

impl Drop for PythonWorker {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
        self.stderr_task.abort();
    }
}

async fn read_stderr(stderr: ChildStderr) -> String {
    let mut reader = BufReader::new(stderr);
    let mut output = String::new();
    let _ = reader.read_to_string(&mut output).await;
    output
}

fn ensure_frame_id(expected: &str, actual: &str) -> Result<()> {
    if expected != actual {
        return Err(anyhow!(
            "worker response id mismatch: expected {expected}, got {actual}"
        ));
    }
    Ok(())
}

pub fn worker_params(pairs: &[(&str, Value)]) -> BTreeMap<String, Value> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_string(), value.clone()))
        .collect()
}

pub fn string_param(value: impl Into<String>) -> Value {
    json!(value.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generated::tushare_fields::dataset;

    #[tokio::test]
    async fn fixture_worker_returns_batch_rows() {
        let mut command = Command::new("python3");
        command.arg("tests/fixtures/tushare_worker_fixture.py");
        let mut worker = PythonWorker::spawn(command).await.unwrap();
        let rows = worker
            .fetch_dataset(
                "req-1",
                dataset("limit_cpt_list").unwrap(),
                &worker_params(&[("trade_date", string_param("20260625"))]),
            )
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("trade_date"), Some(&json!("20260625")));
    }

    #[tokio::test]
    async fn rejects_mismatched_response_id() {
        let mut command = Command::new("python3");
        command.arg("tests/fixtures/tushare_worker_bad_id.py");
        let mut worker = PythonWorker::spawn(command).await.unwrap();
        let error = worker
            .fetch_dataset(
                "req-1",
                dataset("limit_cpt_list").unwrap(),
                &worker_params(&[("trade_date", string_param("20260625"))]),
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("response id mismatch"));
    }
}
