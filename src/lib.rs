//! µQuery as a library: query data files with DuckDB and serve the results
//! over HTTP from your own application.
//!
//! - [`core`]: the query engine. [`DuckDbEngine`](core::duckdb::DuckDbEngine) runs
//!   queries on a pool of DuckDB connections, as Arrow record batches.
//! - [`web`]: the axum [`Router`](axum::Router) of the µQuery HTTP API, built by
//!   [`create_router`](web::routers::create_router), to serve on its own or to nest
//!   in an existing axum application.
//!
//! The `duckdb` and `arrow` crates are re-exported so their versions match the
//! engine's. Depend on uquery with `default-features = false` to leave out the
//! dependencies of the command line binary.
//!
//! ```no_run
//! use std::sync::Arc;
//! use uquery::core::duckdb::DuckDbEngine;
//! use uquery::core::engine::UQueryEngine;
//! use uquery::duckdb::Connection;
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let conn = Connection::open_in_memory()?;
//! // Configure the connection as needed: INSTALL/LOAD extensions, CREATE SECRET, ...
//! conn.execute_batch("INSTALL httpfs; LOAD httpfs;")?;
//! let engine = Arc::new(DuckDbEngine::new(conn, false, 4)?);
//!
//! // Query directly from Rust
//! let result = engine.query("SELECT * FROM 'data.parquet' LIMIT 10")?;
//! println!("{} rows", result.num_rows());
//!
//! // Or serve the HTTP API: CORS disabled, 30s query timeout, no API key
//! let options = uquery::web::routers::RouterOptions {
//!     query_timeout: Some(std::time::Duration::from_secs(30)),
//!     ..Default::default()
//! };
//! let router = uquery::web::routers::create_router(engine, options);
//! let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;
//! axum::serve(listener, router).await?;
//! # Ok(())
//! # }
//! ```

pub mod core;
pub mod web;

pub use arrow;
pub use duckdb;
