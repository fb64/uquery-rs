FROM rust:1.87-slim-bookworm AS builder
RUN apt-get update && apt-get install --no-install-recommends -y build-essential && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY ./Cargo.toml ./Cargo.lock ./
COPY ./src ./src
RUN cargo build --release

FROM debian:bookworm-slim
ARG DUCKDB_VERSION="1.3.0"
LABEL org.opencontainers.image.authors="florian@flob.fr"
LABEL org.opencontainers.image.source="https://github.com/fb64/uquery-rs"
LABEL org.opencontainers.image.description="A lightweight server that provide a simple API to query good old data files (CSV, Json, Parquet ...) with SQL"

## Install DuckDB and preload extensions
RUN apt-get update &&  apt-get install --no-install-recommends -y ca-certificates curl unzip && rm -rf /var/lib/apt/lists/*
RUN curl https://install.duckdb.org | sed "s/VER=.*$/VER=${DUCKDB_VERSION}/" | sh \
    && /root/.duckdb/cli/latest/duckdb :memory: 'INSTALL httpfs' \
    && /root/.duckdb/cli/latest/duckdb :memory: 'INSTALL iceberg' \
    && rm -rf /root/.duckdb/cli

EXPOSE 8080
COPY --from=builder /build/target/release/uquery /usr/local/bin/uquery
ENTRYPOINT ["uquery"]