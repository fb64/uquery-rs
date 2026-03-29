---
sidebar_position: 1
title: Introduction
---

# Introduction

**µQuery** (micro query) is a lightweight HTTP API for querying data files using SQL, powered by [DuckDB](https://duckdb.org/) and built in **Rust**.

Send a SQL query as a plain HTTP request, get results back in the format you need — JSON, CSV, JSON Lines, or Arrow IPC. µQuery handles the rest.

## Key features

- **Format-agnostic**: Query CSV, JSON, Parquet, Excel, and [many more](https://duckdb.org/docs/guides/file_formats/overview) with plain SQL
- **Flexible output**: Stream results as JSON, CSV, JSON Lines, or [Apache Arrow IPC](https://arrow.apache.org/docs/format/IPC.html) — negotiated via the `Accept` header
- **Cloud-native**: Query files directly from S3, GCS, or HTTPS without downloading them first
- **Serverless-ready**: Runs on AWS Lambda and Google Cloud Run with no infrastructure changes
- **Fast startup**: Rust binary with pre-loaded DuckDB extensions — cold starts in milliseconds
- **Connection pooling**: Concurrent queries handled via a configurable pool of DuckDB connections

## How it works

µQuery exposes a single `POST /` endpoint. The request body is a SQL query; the `Accept` header controls the response format:

```bash
curl -X POST http://localhost:8080 \
  -H "Content-Type: text/plain" \
  -H "Accept: application/vnd.apache.arrow.stream" \
  -d "SELECT * FROM 'https://example.com/data.parquet' LIMIT 100"
```

Results are streamed incrementally — the response starts as soon as the first batch is ready, so large queries don't require waiting for full materialization.
