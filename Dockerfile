FROM debian:trixie-slim
ENV DUCKDB_VERSION="1.5.1"
LABEL org.opencontainers.image.authors="florian@flob.fr"
LABEL org.opencontainers.image.source="https://github.com/fb64/uquery-rs"
LABEL org.opencontainers.image.description="A lightweight server that provide a simple API to query good old data files (CSV, Json, Parquet ...) with SQL"

RUN apt-get update && apt-get install --no-install-recommends -y ca-certificates curl unzip && rm -rf /var/lib/apt/lists/*

# Create the runtime user before extension install so they land in its home directory
RUN useradd -r -u 1000 -m -s /bin/false uquery

## Install DuckDB and preload extensions as the runtime user
RUN curl https://install.duckdb.org | sh \
    && cp /root/.duckdb/cli/${DUCKDB_VERSION}/duckdb /usr/local/bin/duckdb \
    && rm -rf /root/.duckdb/cli \
    && su -s /bin/sh uquery -c " \
        duckdb :memory: 'INSTALL httpfs' && \
        duckdb :memory: 'INSTALL iceberg' && \
        duckdb :memory: 'INSTALL ui' && \
        duckdb :memory: 'INSTALL ducklake' && \
        duckdb :memory: 'INSTALL gcs FROM community'" \
    && rm /usr/local/bin/duckdb

EXPOSE 8080
ARG TARGETARCH
COPY bin/uquery-${TARGETARCH} /usr/local/bin/uquery
WORKDIR /tmp
USER uquery
ENTRYPOINT ["uquery"]
