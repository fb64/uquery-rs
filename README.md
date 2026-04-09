<p style="text-align: center;">
  <a href="https://github.com/fb64/uquery-rs/actions"><img src="https://github.com/fb64/uquery-rs/actions/workflows/ci.yml/badge.svg?branch=main" alt="Github Actions Badge"></a>
</p>

# µQuery

**µQuery** (micro query) is a lightweight data querying solution designed for various file formats, including CSV, JSON, and Parquet. Developed in **Rust**, this micro-sized project harnesses the power of [DuckDB](https://duckdb.org/). 

Here’s a quick overview:

1. **Format-Agnostic**: µQuery seamlessly handles diverse data formats, making it easy to query and manipulate your files.
2. **Serverless Deployment**: Deploy µQuery effortlessly on platforms like **AWS Lambda** or **Google Cloud Functions**. No infrastructure headaches—just focus on your data!
3. **Rust-Powered Efficiency**: Built with Rust, µQuery ensures high performance, memory safety, and efficient execution of queries.
4. **DuckDB Integration**: Leverage DuckDB’s embedded SQL engine for direct SQL queries on your data.

In summary, **µQuery** empowers data enthusiasts to work with legacy files efficiently, all while embracing  microservice & serverless approach. Dive into the world of µQuery and unlock seamless data exploration! :rocket:

Full documentation is available here: https://uquery.dev/

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

## Download binary

Pre-built binaries are available on the [GitHub Releases](https://github.com/fb64/uquery-rs/releases/latest) page.

> [!NOTE]  
> Windows build is not available yet.

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
# Optional pre-install some required duckdb extensions for faster startup
uquery --install-extensions
# Start µQuery
uquery
```

### Cargo

[Rust toolchain](https://www.rust-lang.org/tools/install) must be installed

Install with cargo:
```console
cargo install --git https://github.com/fb64/uquery-rs
```

### Docker

µQuery image is available on [Docker Hub](https://hub.docker.com/r/fb64/uquery)


## Usage
### Command-line
```console
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

### Podman / Docker
µQuery docker image is available on [Docker Hub](https://hub.docker.com/r/fb64/uquery)
```
podman run fb64/uquery:latest
#docker run fb64/uquery:latest
```