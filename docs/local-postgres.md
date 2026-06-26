# Local PostgreSQL

Run the local development database with Docker:

```bash
docker compose up -d postgres
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
TUSHARE_TOKEN='dummy' \
cargo run -- init-db
```

For real Tushare cache writes, install the Python runtime dependencies in the environment that runs the CLI:

```bash
python3 -m pip install tushare pandas
```

The worker reads `TUSHARE_TOKEN` from the environment. Do not print or commit the token.
