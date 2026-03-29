---
sidebar_position: 1
title: Query files
---

# Query files

DuckDB supports querying many [file formats](https://duckdb.org/docs/guides/file_formats/overview) including CSV, JSON, Parquet, and Excel. This example shows how to query Parquet files.

[Apache Parquet](https://parquet.apache.org/) is an efficient columnar storage format widely used in data engineering.

## Download Parquet files

Download a [Yellow Taxi](https://www.nyc.gov/site/tlc/about/tlc-trip-record-data.page) record file:

```bash
wget https://d37ci6vzurychx.cloudfront.net/trip-data/yellow_tripdata_2026-01.parquet
```

## Run µQuery with files

Mount the file into the container:

```bash
docker run -p 8080:8080 -v ./yellow_tripdata_2026-01.parquet:/tmp/yellow_tripdata_2026-01.parquet fb64/uquery
```

## Query parquet files

```bash
curl -X POST http://localhost:8080 \
  -H "Accept: application/json" \
  -H "Content-Type: text/plain" \
  -d "select * from read_parquet('/tmp/yellow_tripdata_2026-01.parquet') limit 10"
```

## Query over HTTP(S)

The [DuckDB HTTPFS extension](https://duckdb.org/docs/extensions/httpfs/overview.html) is pre-installed in the Docker image, so you can query remote files directly without downloading them:

```bash
curl -X POST http://localhost:8080 \
  -H "Accept: application/json" \
  -H "Content-Type: text/plain" \
  -d "select * from read_parquet('https://d37ci6vzurychx.cloudfront.net/trip-data/yellow_tripdata_2026-01.parquet') limit 10"
```

This works for any HTTPS-accessible Parquet, CSV, or JSON file.
