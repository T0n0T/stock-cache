//! Long-lived Quack server workflow.

use std::{future::Future, net::Ipv6Addr, path::PathBuf};

use anyhow::{ensure, Context, Result};
use serde_json::json;
use stock_cache::storage::duckdb;

#[derive(Debug)]
pub(crate) struct Options {
    pub(crate) database: Option<PathBuf>,
    pub(crate) bind: String,
    pub(crate) auth_token: Option<String>,
}

pub(crate) fn run(options: Options) -> Result<()> {
    let database = super::database::resolve_database(
        options
            .database
            .map(|path| path.to_string_lossy().into_owned()),
    );
    ensure!(
        PathBuf::from(&database).is_file(),
        "database does not exist: {database}"
    );
    let bind = parse_bind(&options.bind)?;
    let auth_token = options
        .auth_token
        .as_deref()
        .context("serve requires --auth-token or QUACK_AUTH_TOKEN")?;
    ensure!(!auth_token.is_empty(), "Quack auth token must not be empty");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("create signal runtime")?;
    runtime.block_on(async {
        let shutdown = wait_for_shutdown()?;
        let mut server = duckdb::QuackServer::start_read_only(&database, &bind, auth_token)
            .context("start DuckDB Quack server")?;
        println!(
            "{}",
            json!({
                "status": "serving",
                "database": database,
                "bind": bind,
                "url": server.listen_url(),
                "protocol": "quack",
                "read_only": true,
                "authenticated": true,
                "tls": server.listen_url().starts_with("https://"),
                "ssl_fingerprint": server.ssl_fingerprint(),
            })
        );
        let result = shutdown.await;
        server.stop().context("stop DuckDB Quack server")?;
        result
    })
}

fn wait_for_shutdown() -> Result<impl Future<Output = Result<()>>> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut interrupt = signal(SignalKind::interrupt()).context("install SIGINT handler")?;
        let mut terminate = signal(SignalKind::terminate()).context("install SIGTERM handler")?;
        Ok(async move {
            tokio::select! {
                _ = interrupt.recv() => {},
                _ = terminate.recv() => {},
            }
            Ok(())
        })
    }
    #[cfg(not(unix))]
    Ok(async { tokio::signal::ctrl_c().await.context("wait for Ctrl-C") })
}

fn parse_bind(bind: &str) -> Result<String> {
    let bind = bind.trim();
    ensure!(!bind.is_empty(), "--bind must not be empty");
    ensure!(
        !bind.contains(['/', '\\']) && !bind.chars().any(char::is_whitespace),
        "--bind must be host:port"
    );
    let (host, port, bracketed) = if let Some(rest) = bind.strip_prefix('[') {
        let (host, rest) = rest
            .split_once(']')
            .ok_or_else(|| anyhow::anyhow!("--bind has an invalid IPv6 host"))?;
        let port = rest
            .strip_prefix(':')
            .ok_or_else(|| anyhow::anyhow!("--bind must be host:port"))?;
        (host, port, true)
    } else {
        let (host, port) = bind
            .rsplit_once(':')
            .ok_or_else(|| anyhow::anyhow!("--bind must be host:port"))?;
        (host, port, false)
    };
    ensure!(!host.is_empty(), "--bind host must not be empty");
    let port: u16 = port.parse().context("--bind port must be 1..=65535")?;
    ensure!(port != 0, "--bind port must be 1..=65535");
    if bracketed || host.contains(':') {
        ensure!(bracketed, "IPv6 --bind host must be bracketed");
        host.parse::<Ipv6Addr>()
            .context("invalid IPv6 --bind address")?;
        Ok(format!("quack:[{host}]:{port}"))
    } else {
        Ok(format!("quack:{host}:{port}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bind_is_converted_to_quack_uri() {
        assert_eq!(
            parse_bind("127.0.0.1:9494").unwrap(),
            "quack:127.0.0.1:9494"
        );
        assert_eq!(parse_bind("[::1]:9494").unwrap(), "quack:[::1]:9494");
    }

    #[test]
    fn bind_rejects_invalid_values() {
        for bind in [
            "",
            "9494",
            "127.0.0.1:0",
            "127.0.0.1:abc",
            "::1:9494",
            "[localhost]:9494",
            "[127.0.0.1]:9494",
            "local\thost:9494",
        ] {
            assert!(parse_bind(bind).is_err(), "accepted {bind}");
        }
    }
}
