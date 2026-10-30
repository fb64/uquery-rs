---
sidebar_position: 4
title: Configuration
description: "All uQuery CLI flags and UQ_* environment variables: port, connection pool, query timeout, API key, S3 and GCS credentials, Iceberg and DuckDB UI."
---

# Configuration

µQuery is configured via CLI flags or environment variables. All flags have a corresponding `UQ_*` environment variable.

## Server

| Flag | Env var | Default | Description |
|---|---|---|---|
| `--port` | `UQ_PORT` | `8080` | Port to listen on |
| `--addr` | `UQ_ADDR` | `127.0.0.1` | Address to listen on (`0.0.0.0` in the Docker image) |
| `--cors-enabled` | `UQ_CORS_ENABLED` | `false` | Enable permissive CORS (all origins), required for [browser clients](#browser-clients) |
| `--pool-size` | `UQ_POOL_SIZE` | `4` | Number of concurrent DuckDB connections |
| `--query-timeout-secs` | `UQ_QUERY_TIMEOUT` | `30` | Seconds until a query times out (0 = disabled) |
| `--max-query-duration-secs` | `UQ_MAX_QUERY_DURATION` | `600` | Maximum total query time, including streaming the result (0 = no limit) |
| `--memory-limit` | `UQ_MEMORY_LIMIT` | 80% of RAM | DuckDB memory limit, e.g. `4GB` |
| `--threads` | `UQ_THREADS` | CPU cores | Number of DuckDB worker threads |
| `--api-key` | `UQ_API_KEY` | — (open) | Require an API key on queries; empty value generates one |
| `--allow-writes` | `UQ_ALLOW_WRITES` | `false` | Allow statements other than `SELECT` (see [Read-only queries](#read-only-queries)) |

### Listen address

The binary only accepts local connections by default. Use `--addr 0.0.0.0` (or a specific interface address) to accept connections from other hosts, together with an [API key](#api-key). The Docker image sets `UQ_ADDR=0.0.0.0` so that published ports work.

### Pool size

`UQ_POOL_SIZE` controls how many DuckDB connections are kept ready. Requests beyond the pool size queue until a connection is free.

Set it based on your expected query concurrency. Higher values use more memory.

```bash
docker run -p 8080:8080 -e UQ_POOL_SIZE=8 fb64/uquery
```

### Query timeout

`UQ_QUERY_TIMEOUT` sets the maximum time (in seconds) between receiving a request and the first result batch. Queries that take longer return HTTP 408 and are cancelled, freeing their pooled connection. A query is also cancelled when the client disconnects before the first result batch.

Set to `0` to disable:

```bash
docker run -p 8080:8080 -e UQ_QUERY_TIMEOUT=0 fb64/uquery
```

### Resource limits

The query timeout stops at the first result batch: a client can then take as long as it wants to read the result, keeping a pooled connection busy. With a pool of 4 connections, 4 slow clients block every other query. `UQ_MAX_QUERY_DURATION` limits the total time of a query, including streaming its result, to 10 minutes by default; a response still streaming at that point is aborted, so the client gets an error rather than a truncated result. Raise it, or set it to 0 to remove the limit, if trusted clients download results that take longer:

```bash
docker run -p 8080:8080 -e UQ_MAX_QUERY_DURATION=3600 fb64/uquery
```

DuckDB uses 80% of the system memory and all CPU cores by default. `UQ_MEMORY_LIMIT` (e.g. `4GB`) and `UQ_THREADS` lower them, for example when µQuery shares a host with other services. Clients cannot change these settings.

### API key

By default the query API is open. With `--api-key` (or `UQ_API_KEY`), every query must carry the key in one of these headers, otherwise uQuery answers HTTP 401:

```bash
curl -H 'Authorization: Bearer <key>' --data 'SELECT 42' http://localhost:8080
curl -H 'X-API-Key: <key>' --data 'SELECT 42' http://localhost:8080
```

Pass the flag without a value, or set `UQ_API_KEY` to an empty string, to let uQuery generate a random key and print it in the console at startup:

```bash
docker run -p 8080:8080 -e UQ_API_KEY= fb64/uquery
# INFO uquery: API key required, generated key: 3f9c…
```

Use an API key whenever µQuery is reachable by anyone other than you. A generated key changes on every restart; pass your own key to keep it stable. The `/health` endpoint stays open for probes. The [DuckDB UI](#duckdb-ui) proxy cannot check the key, so `--api-key` and `--duckdb-ui` cannot be combined.

### Read-only queries

By default µQuery only runs single `SELECT` queries (including `FROM …`, `WITH …`, `VALUES`, `DESCRIBE` and `SUMMARIZE`). Other statements (`COPY`, `CREATE`, `INSERT`, `ATTACH`, `DETACH`, `USE`, `SET`, `PRAGMA`, `CALL`, `EXPORT`, …) and requests with several statements are rejected with HTTP 403.

All clients share the same DuckDB database, so these statements would let any client write files in the [allowed directories](#allowed-directories), or create tables and views that change what other clients see. Enable them with `--allow-writes` only when all clients are trusted.

Queries using functions or views that reveal the server's settings, secrets, file paths or other clients' activity are rejected with HTTP 403 as well: `duckdb_settings()`, `current_setting()`, `pg_settings`, `duckdb_secrets()`, `which_secret()`, `duckdb_databases()`, `pragma_database_list`, `duckdb_extensions()`, `duckdb_temporary_files()`, `duckdb_external_file_cache()`, `duckdb_logs()` and `duckdb_prepared_statements()`. So are functions that run SQL from a string (`query()`, `query_table()`, `json_execute_serialized_sql()`) or change the server's state (`checkpoint()`, `enable_logging()`, …). Catalog introspection (`duckdb_tables()`, `duckdb_columns()`, `information_schema`, `SHOW TABLES`, `DESCRIBE`) stays available. `--allow-writes` lifts this restriction too.

SQL errors sent to clients don't show the server's paths either: paths in the [allowed directories](#allowed-directories) become relative (`/srv/data/sales.csv` becomes `sales.csv`), the `--db-file` path becomes its file name and the home directory becomes `~`. The server log keeps the full message.

### Browser clients

When CORS is disabled, µQuery rejects requests sent by web browsers (recognized by their `Origin` header) with HTTP 403. Without this check, any web page could make its visitors' browsers send queries to a µQuery server they can reach, even without reading the results. Tools such as `curl`, `httpie` and HTTP client libraries don't send `Origin` and are not affected.

To use µQuery in a web application, call it from your application's server, as in the [Build a Web App](./advanced-tutorials/web-app.md) tutorial: the SQL stays out of the page, and servers don't send `Origin`. Otherwise, enable CORS with `--cors-enabled`, which allows every website, and use an [API key](#api-key).

---

## Database

| Flag | Env var | Default | Description |
|---|---|---|---|
| `--db-file` | `UQ_DB_FILE` | — | DuckDB file to attach (read-only) |
| `--allowed-directories` | `UQ_ALLOWED_DIRECTORIES` | current dir | Comma-separated local directories queries can read |
| `--allowed-urls` | `UQ_ALLOWED_URLS` | `https://`, `s3://`, `gs://`, `gcs://`, `gcss://` | Comma-separated URL prefixes queries can read, or `none` |

### Attached database

Attaches a DuckDB file at startup. All queries run in that database context, making pre-defined macros and views immediately available.

```bash
uquery --db-file custom.db
```

See the [Custom Database](./advanced-tutorials/custom-database.md) tutorial for a full example.

### Allowed directories

Queries can only read files in the allowed directories and URLs; everything else is rejected. By default the allowed directory is the current working directory. Use `--allowed-directories` to choose others:

```bash
uquery --allowed-directories /data/readonly,/tmp/uploads
```

### Allowed URLs

By default queries can read any `https://` URL and any cloud storage bucket (`s3://`, `gs://`, `gcs://`, `gcss://`). Requests are sent from the µQuery server, so a client can reach internal HTTPS services on the server's network, and, with a [credential chain](#cloud-storage), every bucket the server's credentials can read. Use `--allowed-urls` to restrict remote access to the prefixes you need, or `none` to disable it:

```bash
uquery --allowed-urls https://data.example.com/,s3://my-bucket/reports/
uquery --allowed-urls none
```

A prefix naming a host or bucket is completed with a trailing `/`, so `s3://my-bucket` does not also allow `s3://my-bucket-other`. Plain `http://` URLs are not allowed by default.

---

## Cloud Storage

### AWS S3

| Flag | Env var | Description |
|---|---|---|
| `--aws-credential-chain` | `UQ_AWS_CREDENTIAL_CHAIN` | Use the AWS credential chain (IAM role, instance profile, env vars) |

```bash
docker run -p 8080:8080 -e UQ_AWS_CREDENTIAL_CHAIN=true fb64/uquery
```

Once enabled, query S3 files directly:

```sql
SELECT * FROM 's3://my-bucket/data.parquet'
```

See the [AWS Serverless](./advanced-tutorials/cloud-providers/aws-serverless.md) tutorial for full IAM setup.

### Google Cloud Storage

| Flag | Env var | Description |
|---|---|---|
| `--gcs-credential-chain` | `UQ_GCS_CREDENTIAL_CHAIN` | Use the GCP credential chain (Workload Identity, ADC) |
| `--gcs-key-id` | `UQ_GCS_KEY_ID` | GCS HMAC key ID (static credentials) |
| `--gcs-secret` | `UQ_GCS_SECRET` | GCS HMAC secret (static credentials) |

Prefer `UQ_GCS_CREDENTIAL_CHAIN=true` on Cloud Run and GKE — no secrets required:

```bash
docker run -p 8080:8080 -e UQ_GCS_CREDENTIAL_CHAIN=true fb64/uquery
```

Once enabled, query GCS files using the `gcs://` or `gs://` prefix:

```sql
SELECT * FROM 'gcs://my-bucket/data.parquet'
```

See the [GCP Serverless](./advanced-tutorials/cloud-providers/gcp-serverless.md) tutorial for full setup.

---

## Iceberg

| Flag | Env var | Description |
|---|---|---|
| `--ic-catalog-endpoint` | `UQ_ICEBERG_CATALOG_ENDPOINT` | REST catalog endpoint URL |
| `--ic-catalog-name` | `UQ_ICEBERG_CATALOG_NAME` | Catalog name to attach |
| `--ic-user` | `UQ_ICEBERG_USER` | Catalog client ID |
| `--ic-secret` | `UQ_ICEBERG_SECRET` | Catalog client secret |

All four values must be set together to enable Iceberg. Once attached, query Iceberg tables directly by name.

---

## DuckDB UI

| Flag | Env var | Default | Description |
|---|---|---|---|
| `--duckdb-ui` | `UQ_UI_PROXY` | `false` | Enable the DuckDB web UI proxy |
| `--duckdb-ui-port` | `UQ_UI_PORT` | `14213` | Port for the DuckDB UI |

```bash
docker run -p 8080:8080 -p 14213:14213 -e UQ_UI_PROXY=true fb64/uquery
```

The UI proxy listens on the same address as the API (`--addr`). The UI has full access to the DuckDB database and no authentication, so it cannot be combined with `--api-key`.

---

## Verbose logging

Use `-v` (debug) or `-vv` (trace) for more detailed logs:

```bash
uquery -v
```
