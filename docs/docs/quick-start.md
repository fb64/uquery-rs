---
sidebar_position: 2
title: Quick Start
---

# Quick Start

Let's start using **µQuery in less than 5 minutes**.

## Installation
### With Docker

µQuery image is available on [Docker Hub](https://hub.docker.com/r/fb64/uquery)

```shell
# Start µQuery server
docker run -p 8080:8080 fb64/uquery

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
curl --location 'http://localhost:8080' \
--header 'Accept: application/json' \
--header 'Content-Type: application/json' \
--data '{
    "query":"select * from '\''https://raw.githubusercontent.com/duckdb/duckdb-web/main/data/weather.csv'\''"
}'
```

## Explore Options

```shell
uquery -h
Usage: uquery [OPTIONS]

Options:
  -p, --port <PORT>              Port to listen on [env: UQ_PORT=] [default: 8080]
  -a, --addr <ADDR>              Address to listen on [env: UQ_ADDR=] [default: 0.0.0.0]
  -v, --verbose...               Verbose mode
      --gcs-key-id <GCS_KEY_ID>  Google Clous Storage Key ID [env: UQ_GCS_KEY_ID=]
      --gcs-secret <GCS_SECRET>  Google Clous Storage Secret [env: UQ_GCS_SECRET=]
  -d, --db-file <DB_FILE>        DuckDB database file to attach in read only mode and use as default [env: UQ_DB_FILE=]
  -c, --cors-enabled             Enabled permissive CORS [env: UQ_CORS_ENABLED=]
  -h, --help                     Print help
  -V, --version                  Print version
```

