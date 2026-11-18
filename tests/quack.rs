use std::{path::Path, time::Duration};

use assert_cmd::Command;
use tempfile::tempdir;

fn cli(directory: &Path) -> Command {
    let mut command = Command::cargo_bin("stock-cache").unwrap();
    command
        .current_dir(directory)
        .env_remove("DUCKDB_PATH")
        .env_remove("QUACK_BIND")
        .env_remove("QUACK_AUTH_TOKEN")
        .timeout(Duration::from_secs(10));
    command
}

#[test]
fn serve_requires_an_existing_database_and_authentication() {
    let directory = tempdir().unwrap();
    cli(directory.path())
        .args(["serve", "--database", "missing/cache.duckdb"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("database does not exist"));
    assert!(!directory.path().join("missing").exists());

    cli(directory.path())
        .args(["init", "--database", "cache.duckdb"])
        .assert()
        .success();
    cli(directory.path())
        .args(["serve", "--database", "cache.duckdb"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("requires --auth-token"));
    cli(directory.path())
        .args([
            "serve",
            "--database",
            "cache.duckdb",
            "--bind",
            "0.0.0.0:9494",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("requires --auth-token"));
    cli(directory.path())
        .args(["serve", "--database", "cache.duckdb", "--auth-token", ""])
        .assert()
        .failure()
        .stderr(predicates::str::contains("token must not be empty"));
}

#[cfg(unix)]
mod lifecycle {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        net::{TcpListener, TcpStream},
        process::{Child, Command as ProcessCommand, ExitStatus, Stdio},
        sync::mpsc,
        thread,
        time::Instant,
    };

    use assert_cmd::cargo::CommandCargoExt;
    use duckdb_neo::Parameters;
    use serde_json::Value;
    use stock_cache::storage::duckdb;

    struct ServerProcess(Child);

    impl ServerProcess {
        fn start(command: &mut ProcessCommand) -> (Self, Value) {
            let mut server = Self(
                command
                    .stdout(Stdio::piped())
                    .stderr(Stdio::inherit())
                    .spawn()
                    .unwrap(),
            );
            let stdout = server.0.stdout.take().unwrap();
            let (sender, receiver) = mpsc::channel();
            thread::spawn(move || {
                let mut line = String::new();
                BufReader::new(stdout).read_line(&mut line).unwrap();
                let _ = sender.send(line);
            });
            let line = receiver
                .recv_timeout(Duration::from_secs(10))
                .expect("Quack server did not report readiness");
            let ready: Value = serde_json::from_str(&line).expect("Quack server failed to start");
            assert_eq!(ready["status"], "serving");
            assert_eq!(ready["read_only"], true);
            assert_eq!(ready["authenticated"], true);
            assert!(!line.contains("integration'token"));
            (server, ready)
        }

        fn stop(&mut self, signal: &str) -> ExitStatus {
            assert!(ProcessCommand::new("kill")
                .args([signal, &self.0.id().to_string()])
                .status()
                .unwrap()
                .success());
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if let Some(status) = self.0.try_wait().unwrap() {
                    return status;
                }
                assert!(Instant::now() < deadline, "Quack server did not stop");
                thread::sleep(Duration::from_millis(20));
            }
        }
    }

    impl Drop for ServerProcess {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[test]
    fn serve_queries_read_only_cache_and_releases_locks_on_shutdown() {
        let directory = tempdir().unwrap();
        let database = directory.path().join("cache.duckdb");
        let database = database.to_str().unwrap();
        duckdb::with_connection(database, |connection| {
            connection.execute(
                "CREATE TABLE network_probe AS SELECT 42::BIGINT AS answer",
                Parameters::None,
            )?;
            duckdb::checkpoint(connection)
        })
        .unwrap();

        for signal in ["-TERM", "-INT"] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            drop(listener);
            let endpoint = format!("quack:{address}");
            let mut command = ProcessCommand::cargo_bin("stock-cache").unwrap();
            command
                .current_dir(directory.path())
                .env_remove("DUCKDB_PATH")
                .env_remove("QUACK_BIND")
                .env_remove("QUACK_AUTH_TOKEN")
                .arg("serve");
            if signal == "-TERM" {
                command.args([
                    "--database",
                    database,
                    "--bind",
                    &address.to_string(),
                    "--auth-token",
                    "integration'token",
                ]);
            } else {
                std::fs::write(
                    directory.path().join(".env"),
                    format!(
                        "DUCKDB_PATH={database}\nQUACK_BIND={address}\nQUACK_AUTH_TOKEN=integration'token\n"
                    ),
                )
                .unwrap();
            }
            let (mut server, ready) = ServerProcess::start(&mut command);
            assert_eq!(ready["tls"], false);
            assert!(ready["ssl_fingerprint"].is_null());
            duckdb::with_connection(":memory:", |connection| {
                for auth in ["", " (TOKEN 'wrong-token')"] {
                    let error = connection
                        .execute(
                            format!("ATTACH '{endpoint}' AS rejected{auth}"),
                            Parameters::None,
                        )
                        .unwrap_err();
                    assert!(error.to_string().to_lowercase().contains("auth"), "{error}");
                }
                connection.execute(
                    format!("ATTACH '{endpoint}' AS cache (TOKEN 'integration''token')"),
                    Parameters::None,
                )?;
                let mut result = connection.query(
                    "SELECT answer FROM cache.main.network_probe",
                    Parameters::None,
                )?;
                let chunk = result.next_chunk()?.unwrap();
                assert_eq!(chunk.get_vector_at::<i64>(0)?.reader()?.get(0)?, Some(&42));
                drop(result);
                let error = connection
                    .execute(
                        "UPDATE cache.main.network_probe SET answer = 0",
                        Parameters::None,
                    )
                    .unwrap_err();
                assert!(error.to_string().contains("read-only"), "{error}");
                Ok(())
            })
            .unwrap();
            let error = duckdb::with_connection(database, |_| Ok(())).unwrap_err();
            assert!(format!("{error:#}").contains("lock"), "{error:#}");

            assert!(server.stop(signal).success());
            assert!(TcpStream::connect_timeout(&address, Duration::from_secs(1)).is_err());
            duckdb::with_connection(database, |connection| {
                connection.execute("UPDATE network_probe SET answer = 42", Parameters::None)?;
                Ok(())
            })
            .unwrap();
        }
    }

    #[test]
    fn network_bind_uses_tls_and_supplies_a_working_certificate_fingerprint() {
        let directory = tempdir().unwrap();
        cli(directory.path())
            .args(["init", "--database", "cache.duckdb"])
            .assert()
            .success();
        let listener = TcpListener::bind("0.0.0.0:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let mut command = ProcessCommand::cargo_bin("stock-cache").unwrap();
        command
            .current_dir(directory.path())
            .env_remove("DUCKDB_PATH")
            .env_remove("QUACK_BIND")
            .env_remove("QUACK_AUTH_TOKEN")
            .args([
                "serve",
                "--database",
                "cache.duckdb",
                "--bind",
                &format!("0.0.0.0:{port}"),
                "--auth-token",
                "integration'token",
            ]);
        let (mut server, ready) = ServerProcess::start(&mut command);
        assert_eq!(ready["tls"], true);
        assert_eq!(ready["url"], format!("https://0.0.0.0:{port}"));
        let fingerprint = ready["ssl_fingerprint"].as_str().unwrap();
        assert!(!fingerprint.is_empty());
        duckdb::with_connection(":memory:", |connection| {
            connection.execute(
                format!(
                    "ATTACH 'quack:127.0.0.1:{port}' AS cache \
                     (TOKEN 'integration''token', SSL_FINGERPRINT '{fingerprint}')"
                ),
                Parameters::None,
            )?;
            let mut result = connection.query(
                "SELECT count(*) FROM cache.main.cache_trade_calendar",
                Parameters::None,
            )?;
            let chunk = result.next_chunk()?.unwrap();
            assert_eq!(chunk.get_vector_at::<i64>(0)?.reader()?.get(0)?, Some(&0));
            Ok(())
        })
        .unwrap();
        assert!(server.stop("-TERM").success());
    }
}
