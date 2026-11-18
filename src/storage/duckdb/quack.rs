//! DuckDB's native Quack server lifecycle.

use anyhow::{ensure, Context, Result};
use duckdb_neo::{
    connection::Connection,
    environment::{Environment, StorageLocation},
    instance::{AttachBuilder, Instance},
    Parameters,
};

pub struct QuackServer {
    _environment: Environment,
    _instance: Instance,
    connection: Connection,
    uri: String,
    listen_url: String,
    ssl_fingerprint: Option<String>,
    running: bool,
}

impl QuackServer {
    pub fn start_read_only(database: &str, bind: &str, auth_token: &str) -> Result<Self> {
        ensure!(!auth_token.is_empty(), "Quack auth token must not be empty");
        let environment = Environment::new().context("create DuckDB environment")?;
        let instance = Instance::new(&environment).context("create DuckDB instance")?;
        instance
            .attach(
                AttachBuilder::new(StorageLocation::OnDisk(database.to_owned()))
                    .options([("read_only", "true")])
                    .make_default(),
            )
            .context("attach DuckDB database")?;
        let connection = instance.connect().context("connect DuckDB database")?;
        let mut server = Self {
            _environment: environment,
            _instance: instance,
            connection,
            uri: bind.to_owned(),
            listen_url: String::new(),
            ssl_fingerprint: None,
            running: false,
        };
        let mut result = server
            .connection
            .query(
                format!(
                    "CALL quack_serve({}, token = {}, allow_other_hostname = true)",
                    sql_string_literal(&server.uri),
                    sql_string_literal(auth_token)
                ),
                Parameters::None,
            )
            .context("CALL quack_serve")?;
        server.running = true;
        let chunk = result
            .next_chunk()?
            .context("Quack returned no server address")?;
        server.listen_url = chunk
            .get_vector_at::<String>(1)?
            .reader()?
            .get(0)?
            .context("Quack returned a NULL server address")?
            .to_owned();
        drop(result);
        let mut result = server.connection.query(
            format!(
                "SELECT info['ssl_fingerprint'] FROM quack_server_list() WHERE listen_uri = {}",
                sql_string_literal(&server.uri)
            ),
            Parameters::None,
        )?;
        let chunk = result
            .next_chunk()?
            .context("Quack server is missing from server list")?;
        server.ssl_fingerprint = chunk
            .get_vector_at::<String>(0)?
            .reader()?
            .get(0)?
            .map(str::to_owned);
        drop(result);
        Ok(server)
    }

    pub fn listen_url(&self) -> &str {
        &self.listen_url
    }

    pub fn ssl_fingerprint(&self) -> Option<&str> {
        self.ssl_fingerprint.as_deref()
    }

    pub fn stop(&mut self) -> Result<()> {
        if !self.running {
            return Ok(());
        }
        self.connection
            .execute(
                format!("CALL quack_stop({})", sql_string_literal(&self.uri)),
                Parameters::None,
            )
            .map(|_| ())
            .context("CALL quack_stop")?;
        self.running = false;
        Ok(())
    }
}

impl Drop for QuackServer {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn sql_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
