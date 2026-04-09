---
sidebar_position: 2
title: Quick Start
---

# Quick Start

Let's start using **µQuery in less than 5 minutes**.

## Installation

```shell 
curl -fsSL https://install.uquery.dev | sh
```

```shell
# -f fail fast
# -s silent
# -L follow redirects https://install.uquery.dev --> https://uquery.dev/install.sh
# -S show error messages
```
 

### With Docker

µQuery image is available on [Docker Hub](https://hub.docker.com/r/fb64/uquery)

```shell
# Start µQuery server
docker run -p 8080:8080 fb64/uquery

```

### Download binary

Pre-built binaries are available on the [GitHub Releases](https://github.com/fb64/uquery-rs/releases/latest) page.

:::note 

Windows build is not available yet.

:::

| Platform | Architecture | Asset |
|---|---|---|
| macOS | Apple Silicon | `uquery-aarch64-apple-darwin.tar.gz` |
| macOS | Intel | `uquery-x86_64-apple-darwin.tar.gz` |
| Linux | x86_64 | `uquery-x86_64-unknown-linux-gnu.tar.gz` |
| Linux | ARM64 | `uquery-aarch64-unknown-linux-gnu.tar.gz` |
| Linux | x86 (32-bit) | `uquery-i686-unknown-linux-gnu.tar.gz` |

Once the binary is downloaded and added to your `PATH` you could run 

```shell
# macOS (Apple Silicon)
curl -fsSL https://github.com/fb64/uquery-rs/releases/latest/download/uquery-aarch64-apple-darwin.tar.gz | tar -xz
# macOS (Intel)
curl -fsSL https://github.com/fb64/uquery-rs/releases/latest/download/uquery-x86_64-apple-darwin.tar.gz | tar -xz
# Linux (x86_64)
curl -fsSL https://github.com/fb64/uquery-rs/releases/latest/download/uquery-x86_64-unknown-linux-gnu.tar.gz | tar -xz
# Linux (ARM64)
curl -fsSL https://github.com/fb64/uquery-rs/releases/latest/download/uquery-aarch64-unknown-linux-gnu.tar.gz | tar -xz
# Linux (x86 32-bit)
curl -fsSL https://github.com/fb64/uquery-rs/releases/latest/download/uquery-i686-unknown-linux-gnu.tar.gz | tar -xz

# Move to a directory in your PATH
mv uquery /usr/local/bin/
# Optional pre-install some required duckdb extensions
uquery --install-extensions
# Start µQuery
uquery
```

### With Cargo

[Rust toolchain](https://www.rust-lang.org/tools/install) must be installed

Install with cargo:
```shell
# Install µQuery binary
cargo install --git https://github.com/fb64/uquery-rs
# Start µQuery
uquery
```


## Simple Query example

```shell
#Run a query
curl -X POST http://localhost:8080 \
  -H "Accept: application/json" \
  -H "Content-Type: text/plain" \
  -d "select * from 'https://raw.githubusercontent.com/duckdb/duckdb-web/main/data/weather.csv'"
```

## Response Formats

Control the output format with the `Accept` header:

| Accept header | Format |
|---|---|
| `application/json` | JSON array (default) |
| `application/jsonlines` | JSON Lines (one object per line) |
| `text/csv` | CSV with header row |
| `application/vnd.apache.arrow.stream` | Apache Arrow IPC stream |

```shell
# CSV output
curl -X POST http://localhost:8080 \
  -H "Content-Type: text/plain" \
  -H "Accept: text/csv" \
  -d "select * from 'https://raw.githubusercontent.com/duckdb/duckdb-web/main/data/weather.csv'"

# Arrow IPC output
curl -X POST http://localhost:8080 \
  -H "Content-Type: text/plain" \
  -H "Accept: application/vnd.apache.arrow.stream" \
  -d "select * from 'https://raw.githubusercontent.com/duckdb/duckdb-web/main/data/weather.csv'" \
  --output result.arrow
```

See [Response Formats](./response-formats.md) for details.

## Health check

```shell
curl http://localhost:8080/health
```

Returns `200 OK` when the server is ready.

## Explore Options

```shell
uquery -h
Usage: uquery [OPTIONS]

Options:
  -p, --port <PORT>
          Port to listen on [env: UQ_PORT=] [default: 8080]
  -a, --addr <ADDR>
          Address to listen on [env: UQ_ADDR=] [default: 0.0.0.0]
  -v, --verbose...
          Verbose mode
      --gcs-key-id <GCS_KEY_ID>
          Google Cloud Storage Key ID [env: UQ_GCS_KEY_ID=]
      --gcs-secret <GCS_SECRET>
          Google Cloud Storage Secret [env: UQ_GCS_SECRET=]
      --gcs-credential-chain
          Enable GCS Credential Chain [env: UQ_GCS_CREDENTIAL_CHAIN=]
  -d, --db-file <DB_FILE>
          DuckDB database file to attach in read only mode and use as default [env: UQ_DB_FILE=]
  -c, --cors-enabled
          Enabled permissive CORS [env: UQ_CORS_ENABLED=]
      --aws-credential-chain
          Enable AWS Credential Chain [env: UQ_AWS_CREDENTIAL_CHAIN=]
      --duckdb-ui
          Enable DuckDB UI Proxy [env: UQ_UI_PROXY=]
      --duckdb-ui-port <DUCKDB_UI_PORT>
          DuckDB UI Port [env: UQ_UI_PORT=] [default: 14213]
      --ic-catalog-endpoint <IC_CATALOG_ENDPOINT>
          Iceberg Catalog Endpoint [env: UQ_ICEBERG_CATALOG_ENDPOINT=]
      --ic-catalog-name <IC_CATALOG_NAME>
          Iceberg Catalog name [env: UQ_ICEBERG_CATALOG_NAME=]
      --ic-user <IC_USER>
          Iceberg User [env: UQ_ICEBERG_USER=]
      --ic-secret <IC_SECRET>
          [env: UQ_ICEBERG_SECRET=]
      --allowed-directories <ALLOWED_DIRECTORIES>
          [env: UQ_ALLOWED_DIRECTORIES=]
      --pool-size <POOL_SIZE>
          Number of pre-cloned DuckDB connections kept in the pool [env: UQ_POOL_SIZE=] [default: 4]
      --query-timeout-secs <QUERY_TIMEOUT_SECS>
          Maximum query execution time in seconds (0 = no timeout) [env: UQ_QUERY_TIMEOUT=] [default: 30]
      --install-extensions
          Install all DuckDB extensions and exit. Use this once after installation to pre-download extensions so the server starts without network access [env: UQ_INSTALL_EXTENSIONS=]
  -h, --help
          Print help
  -V, --version
          Print version

```

See [Configuration](./configuration.md) for a full reference of all options.

