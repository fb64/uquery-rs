---
sidebar_position: 5
title: Rust Library
description: Embed the uQuery engine or its HTTP API in your own Rust application.
---

# Rust Library

µQuery can be used as a Rust library, to run queries from your code or to serve the µQuery HTTP API from your own application.

```toml
[dependencies]
uquery = { git = "https://github.com/fb64/uquery-rs", default-features = false }
tokio = { version = "1", features = ["full"] }
axum = "0.8"
```

`default-features = false` leaves out the dependencies of the `uquery` command line binary (argument parsing, DuckDB UI proxy, self-update).

## Query from Rust

`DuckDbEngine` runs queries on a pool of DuckDB connections cloned from the one you give it. Configure that connection first: install and load extensions, create secrets, attach databases.

```rust
use uquery::core::duckdb::DuckDbEngine;
use uquery::core::engine::UQueryEngine;
use uquery::duckdb::Connection;

let conn = Connection::open_in_memory()?;
conn.execute_batch("INSTALL httpfs; LOAD httpfs;")?;
// pool of 4 connections, no attached database
let engine = DuckDbEngine::new(conn, false, 4)?;

let result = engine.query("SELECT * FROM 'https://raw.githubusercontent.com/duckdb/duckdb-web/main/data/weather.csv'")?;
println!("{} rows", result.num_rows());
for batch in result.batches {
    // arrow::record_batch::RecordBatch
}
```

`query` collects the whole result in memory. To stream large results, call `prepare` and pass a `RecordBatchConsumer` to `execute`. The query handle also gives a `canceller` to stop it from another thread.

Errors are returned as `uquery::core::error::EngineError`, which implements `std::error::Error`: `DuckDb` and `Arrow` wrap the underlying error, `Cancelled` is a query cancelled before it started, and `Consumer` carries any error returned by your own `RecordBatchConsumer`.

The `duckdb` and `arrow` crates are re-exported as `uquery::duckdb` and `uquery::arrow`, so their versions always match the engine's.

### Attached database

To query a database file by default, as `uquery --db-file` does, attach it under `UQ_ATTACHED_DB_NAME` and pass `true` for `attached`:

```rust
use uquery::core::duckdb::UQ_ATTACHED_DB_NAME;

conn.execute_batch(&format!("ATTACH 'my.db' AS {UQ_ATTACHED_DB_NAME} (READ_ONLY);"))?;
let engine = DuckDbEngine::new(conn, true, 4)?;
```

## Serve the HTTP API

`create_router` returns the axum `Router` of the µQuery API, with the same response formats, compression, timeout and API key handling as the server. Serve it alone or nest it in your application.

```rust
use std::sync::Arc;
use std::time::Duration;

let router = uquery::web::routers::create_router(
    Arc::new(engine),
    false,                          // permissive CORS
    Some(Duration::from_secs(30)),  // query timeout
    Some("my-secret-key".into()),   // API key, None for an open API
);
let app = axum::Router::new().nest("/uquery", router);
let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
axum::serve(listener, app).await?;
```

## Other languages

There are no bindings for other languages. Run the µQuery server and call its HTTP API: request `application/vnd.apache.arrow.stream` to load results directly into pyarrow, polars, Apache Arrow JS or Arrow Java (see [Response Formats](./response-formats.md)). To use DuckDB in-process, use its official clients for these languages.
