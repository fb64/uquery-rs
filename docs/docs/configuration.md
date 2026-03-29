---
sidebar_position: 4
title: Configuration
---

# Configuration

µQuery is configured via CLI flags or environment variables. All flags have a corresponding `UQ_*` environment variable.

## Server

| Flag | Env var | Default | Description |
|---|---|---|---|
| `--port` | `UQ_PORT` | `8080` | Port to listen on |
| `--addr` | `UQ_ADDR` | `0.0.0.0` | Address to listen on |
| `--cors-enabled` | `UQ_CORS_ENABLED` | `false` | Enable permissive CORS (all origins) |
| `--pool-size` | `UQ_POOL_SIZE` | `4` | Number of concurrent DuckDB connections |
| `--query-timeout` | `UQ_QUERY_TIMEOUT` | `30` | Seconds until a query times out (0 = disabled) |

### Pool size

`UQ_POOL_SIZE` controls how many DuckDB connections are kept ready. Requests beyond the pool size queue until a connection is free.

Set it based on your expected query concurrency. Higher values use more memory.

```bash
docker run -p 8080:8080 -e UQ_POOL_SIZE=8 fb64/uquery
```

### Query timeout

`UQ_QUERY_TIMEOUT` sets the maximum time (in seconds) between receiving a request and the first result batch. Queries that take longer return HTTP 408.

Set to `0` to disable:

```bash
docker run -p 8080:8080 -e UQ_QUERY_TIMEOUT=0 fb64/uquery
```

---

## Database

| Flag | Env var | Default | Description |
|---|---|---|---|
| `--db-file` | `UQ_DB_FILE` | — | DuckDB file to attach (read-only) |
| `--allowed-directories` | `UQ_ALLOWED_DIRECTORIES` | current dir + cloud prefixes | Restrict file access to specific paths |

### Attached database

Attaches a DuckDB file at startup. All queries run in that database context, making pre-defined macros and views immediately available.

```bash
uquery --db-file custom.db
```

See the [Custom Database](./advanced-tutorials/custom-database.md) tutorial for a full example.

### Allowed directories

By default µQuery allows access to the current working directory and cloud storage prefixes (`s3://`, `gcs://`, etc.). Use `--allowed-directories` to restrict or expand this:

```bash
uquery --allowed-directories /data/readonly,/tmp/uploads
```

Setting this disables all external access not explicitly listed.

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

---

## Verbose logging

Use `-v` (debug) or `-vv` (trace) for more detailed logs:

```bash
uquery -v
```
