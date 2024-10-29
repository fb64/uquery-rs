FROM rust:1.81-slim-bookworm AS builder
RUN apt-get update && apt-get install -y build-essential
WORKDIR /build
COPY ./Cargo.toml ./Cargo.lock ./
COPY ./src ./src
RUN cargo build --release

FROM debian:bookworm-slim
ARG TARGETPLATFORM
ARG DUCKDB_VERSION="1.1.1"
LABEL org.opencontainers.image.authors="florian@flob.fr"
LABEL org.opencontainers.image.source="https://github.com/fb64/uquery-rs"
LABEL org.opencontainers.image.description="A lightweight server that provide a simple API to query good old data files (CSV, Json, Parquet ...) with SQL"

## Install DuckDB and preload extensions
RUN apt-get update && apt-get install -y curl unzip
RUN if [ "$TARGETPLATFORM" = "linux/amd64" ]; then ARCHITECTURE=amd64; elif [ "$TARGETPLATFORM" = "linux/arm64" ]; then ARCHITECTURE=aarch64; else ARCHITECTURE=amd64; fi \
    && curl -sS -L -O --output-dir /tmp/ --create-dirs "https://github.com/duckdb/duckdb/releases/download/v${DUCKDB_VERSION}/duckdb_cli-linux-${ARCHITECTURE}.zip" \
    && unzip /tmp/duckdb_cli-linux-${ARCHITECTURE}.zip -d /usr/bin \
    && duckdb :memory 'INSTALL HTTPFS' \
    && rm -f /tmp/duckdb_cli-linux-${ARCHITECTURE}.zip /usr/bin/duckdb

EXPOSE 8080
COPY --from=builder /build/target/release/uquery /usr/local/bin/uquery
ENTRYPOINT ["uquery"]