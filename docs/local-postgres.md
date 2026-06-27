# Local PostgreSQL

Run the local development database with Docker:

```bash
docker compose -f deploy/docker-compose.yml up -d postgres
```

The container uses `postgres:alpine` and stores data under:

```text
runtime/postgres/
```

Default DSN:

```text
postgres://stock_cache:stock_cache@localhost:5432/stock_cache
```

Initialize the schema:

```bash
POSTGRES_DSN='postgres://stock_cache:stock_cache@localhost:5432/stock_cache' \
cargo run -- init-db
```

For real Tushare cache writes, install the Python worker dependencies:

```bash
uv sync --project python
```

By default the Rust CLI runs `python/.venv/bin/python python/tushare_worker.py`. The worker reads `TUSHARE_TOKEN` from the environment. Do not print or commit the token.
