<p style="text-align: center;">
  <a href="https://github.com/fb64/uquery-rs/actions"><img src="https://github.com/fb64/uquery-rs/actions/workflows/ci.yml/badge.svg?branch=main" alt="Github Actions Badge"></a>
</p>

# µQuery

**µQuery** (micro query) is a lightweight data querying solution designed for various file formats, including CSV, JSON, and Parquet. Developed in **Rust**, this micro-sized project harnesses the power of [DuckDB](https://duckdb.org/). Here’s a quick overview:

1. **Format-Agnostic**: µQuery seamlessly handles diverse data formats, making it easy to query and manipulate your files.
2. **Serverless Deployment**: Deploy µQuery effortlessly on platforms like **AWS Lambda** or **Google Cloud Functions**. No infrastructure headaches—just focus on your data!
3. **Rust-Powered Efficiency**: Built with Rust, µQuery ensures high performance, memory safety, and efficient execution of queries.
4. **DuckDB Integration**: Leverage DuckDB’s embedded SQL engine for direct SQL queries on your data.

In summary, **µQuery** empowers data enthusiasts to work with legacy files efficiently, all while embracing  microservice & serverless approach. Dive into the world of µQuery and unlock seamless data exploration! :rocket:

Full documentation is available here: https://uquery.flob.fr/

## Installation

[Rust toolchain](https://www.rust-lang.org/tools/install) must be installed

Install with cargo:
```console
cargo install --git https://github.com/fb64/uquery-rs
```

## Usage
### Command-line
```console
$uquery -h
Usage: uquery [OPTIONS]

Options:
  -p, --port <PORT>              Port to listen on [env: UQ_PORT=] [default: 8080]
  -a, --addr <ADDR>              Address to listen on [env: UQ_ADDR=] [default: 0.0.0.0]
  -v, --verbose...               Verbose mode
      --gcs-key-id <GCS_KEY_ID>  Google Clous Storage Key ID [env: UQ_GCS_KEY_ID=]
      --gcs-secret <GCS_SECRET>  Google Clous Storage Secret [env: UQ_GCS_SECRET=]
  -d, --db-file <DB_FILE>        DuckDB database file to attach in read only mode and use as default [env: UQ_DB_FILE=]
  -c, --cors-enabled             Enabled permissive CORS [env: UQ_CORS_ENABLED=]
      --aws-credential-chain     Enable AWS Credential Chain [env: UQ_AWS_CREDENTIAL_CHAIN=]
  -h, --help                     Print help
  -V, --version                  Print version
```

### Docker
µQuery docker image is available on [Docker Hub](https://hub.docker.com/r/fb64/uquery)
```
docker run fb64/uquery
```