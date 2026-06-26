use std::{collections::HashMap, env, fs, path::Path};

use anyhow::{anyhow, Context, Result};
use serde::Serialize;

const DEFAULT_INDEX_LIST_PATH: &str = "runtime/default-indexes.csv";

#[derive(Debug, Clone, Serialize)]
pub struct Config {
    #[serde(skip_serializing)]
    pub postgres_dsn: String,
    #[serde(skip_serializing)]
    pub tushare_token: String,
    pub postgres_dsn_set: bool,
    pub tushare_token_set: bool,
    pub index_list_path: String,
    pub default_lookback_trading_days: u32,
    pub max_concurrency: u32,
    pub request_timeout_seconds: u64,
    pub max_retries: u32,
    pub retry_base_delay_seconds: f64,
    pub write_batch_size: usize,
    pub log_level: String,
}

#[derive(Debug, Clone)]
pub struct EnvFile {
    values: HashMap<String, String>,
}

impl EnvFile {
    fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }
}

pub fn load_env_file(path: Option<&Path>) -> Result<Option<EnvFile>> {
    let Some(path) = path else {
        return Ok(None);
    };
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read env file {}", path.display()))?;
    let mut values = HashMap::new();
    for (line_number, raw_line) in contents.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(anyhow!(
                "invalid env file line {} in {}: expected KEY=VALUE",
                line_number + 1,
                path.display()
            ));
        };
        let key = key.trim();
        if key.is_empty() {
            return Err(anyhow!(
                "invalid env file line {} in {}: empty key",
                line_number + 1,
                path.display()
            ));
        }
        values.insert(key.to_string(), unquote_value(value.trim()).to_string());
    }
    Ok(Some(EnvFile { values }))
}

fn unquote_value(value: &str) -> &str {
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        let first = bytes[0];
        let last = bytes[value.len() - 1];
        if (first == b'\'' && last == b'\'') || (first == b'"' && last == b'"') {
            return &value[1..value.len() - 1];
        }
    }
    value
}

impl Config {
    pub fn from_env(env_file: Option<&EnvFile>) -> Result<Self> {
        let postgres_dsn = required("POSTGRES_DSN", env_file)?;
        let tushare_token = required("TUSHARE_TOKEN", env_file)?;
        let index_list_path = value("INDEX_LIST_PATH", env_file)
            .unwrap_or_else(|| DEFAULT_INDEX_LIST_PATH.to_string());

        Ok(Self {
            postgres_dsn,
            tushare_token,
            postgres_dsn_set: true,
            tushare_token_set: true,
            index_list_path,
            default_lookback_trading_days: parse_or_default(
                "DEFAULT_LOOKBACK_TRADING_DAYS",
                env_file,
                90,
            )?,
            max_concurrency: parse_or_default("MAX_CONCURRENCY", env_file, 4)?,
            request_timeout_seconds: parse_or_default("REQUEST_TIMEOUT_SECONDS", env_file, 30)?,
            max_retries: parse_or_default("MAX_RETRIES", env_file, 3)?,
            retry_base_delay_seconds: parse_or_default("RETRY_BASE_DELAY_SECONDS", env_file, 1.0)?,
            write_batch_size: parse_or_default("WRITE_BATCH_SIZE", env_file, 1000)?,
            log_level: value("LOG_LEVEL", env_file).unwrap_or_else(|| "info".to_string()),
        })
    }

    pub fn print_json(&self) -> Result<()> {
        println!("{}", serde_json::to_string_pretty(self)?);
        Ok(())
    }
}

fn required(key: &str, env_file: Option<&EnvFile>) -> Result<String> {
    value(key, env_file).ok_or_else(|| anyhow!("missing required environment variable {key}"))
}

fn value(key: &str, env_file: Option<&EnvFile>) -> Option<String> {
    match env::var(key) {
        Ok(value) => Some(value),
        Err(env::VarError::NotPresent) => {
            env_file.and_then(|file| file.get(key)).map(str::to_string)
        }
        Err(env::VarError::NotUnicode(_)) => None,
    }
}

fn parse_or_default<T>(key: &str, env_file: Option<&EnvFile>, default: T) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let Some(raw) = value(key, env_file) else {
        return Ok(default);
    };
    raw.parse::<T>()
        .map_err(|err| anyhow!("invalid value for {key}: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unquotes_single_and_double_quoted_values() {
        assert_eq!(unquote_value("'abc'"), "abc");
        assert_eq!(unquote_value("\"abc\""), "abc");
        assert_eq!(unquote_value("abc"), "abc");
    }
}
