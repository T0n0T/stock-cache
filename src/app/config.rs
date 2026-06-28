use std::{collections::HashMap, env, fs, path::Path};

use anyhow::{anyhow, Context, Result};
const DEFAULT_INDEX_LIST_PATH: &str = "config/default-indexes.csv";

#[derive(Debug, Clone)]
pub struct Config {
    pub postgres_dsn: String,
    pub tushare_token: String,
    pub index_list_path: String,
    pub default_lookback_trading_days: u32,
    pub max_concurrency: u32,
    pub request_timeout_seconds: u64,
    pub max_retries: u32,
    pub retry_base_delay_seconds: f64,
    pub write_batch_size: usize,
    pub calc_chunk_size: Option<usize>,
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
    pub fn from_env_with_options(
        env_file: Option<&EnvFile>,
        require_tushare_token: bool,
    ) -> Result<Self> {
        let postgres_dsn = required("POSTGRES_DSN", env_file)?;
        let tushare_token = if require_tushare_token {
            required("TUSHARE_TOKEN", env_file)?
        } else {
            value("TUSHARE_TOKEN", env_file).unwrap_or_default()
        };
        let index_list_path = value("INDEX_LIST_PATH", env_file)
            .unwrap_or_else(|| DEFAULT_INDEX_LIST_PATH.to_string());

        Ok(Self {
            postgres_dsn,
            tushare_token,
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
            calc_chunk_size: parse_optional_positive_usize("CALC_CHUNK_SIZE", env_file)?,
            log_level: value("LOG_LEVEL", env_file).unwrap_or_else(|| "info".to_string()),
        })
    }

    pub fn print_human_readable(&self) {
        println!("Configuration");
        println!("  Secrets");
        println!("    POSTGRES_DSN: {}", self.postgres_dsn);
        println!("    TUSHARE_TOKEN: {}", self.tushare_token);
        println!();
        println!("  Paths");
        println!("    INDEX_LIST_PATH: {}", self.index_list_path);
        println!();
        println!("  Runtime");
        println!(
            "    DEFAULT_LOOKBACK_TRADING_DAYS: {}",
            self.default_lookback_trading_days
        );
        println!("    MAX_CONCURRENCY: {}", self.max_concurrency);
        println!(
            "    REQUEST_TIMEOUT_SECONDS: {}",
            self.request_timeout_seconds
        );
        println!("    MAX_RETRIES: {}", self.max_retries);
        println!(
            "    RETRY_BASE_DELAY_SECONDS: {}",
            self.retry_base_delay_seconds
        );
        println!("    WRITE_BATCH_SIZE: {}", self.write_batch_size);
        println!(
            "    CALC_CHUNK_SIZE: {}",
            self.calc_chunk_size
                .map(|value| value.to_string())
                .unwrap_or_else(|| "auto".to_string())
        );
        println!("    LOG_LEVEL: {}", self.log_level);
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

fn parse_optional_positive_usize(key: &str, env_file: Option<&EnvFile>) -> Result<Option<usize>> {
    let Some(raw) = value(key, env_file) else {
        return Ok(None);
    };
    let parsed = raw
        .parse::<usize>()
        .map_err(|err| anyhow!("invalid value for {key}: {err}"))?;
    if parsed == 0 {
        return Err(anyhow!("{key} must be greater than 0"));
    }
    Ok(Some(parsed))
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
