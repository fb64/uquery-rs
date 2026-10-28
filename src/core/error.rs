use arrow::error::ArrowError;
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use std::fmt;

#[derive(Debug, Clone)]
pub struct UQueryError {
    pub(crate) status_code: u16,
    pub(crate) title: String,
    pub(crate) detail: String,
}

impl Serialize for UQueryError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("UQueryError", 3)?;
        state.serialize_field("status", &self.status_code)?;
        state.serialize_field("title", &self.title)?;
        state.serialize_field("detail", &self.detail)?;
        state.end()
    }
}

/// Error returned by the query engine and by [`RecordBatchConsumer`]s.
///
/// [`RecordBatchConsumer`]: crate::core::engine::RecordBatchConsumer
#[derive(Debug)]
#[non_exhaustive]
pub enum EngineError {
    /// DuckDB failed to open a connection, or rejected or failed the query.
    DuckDb(duckdb::Error),
    /// Arrow failed to encode a result batch.
    Arrow(ArrowError),
    /// The query was cancelled before it started or while streaming its result.
    Cancelled,
    /// The query is not allowed, e.g. a write statement on a read-only engine.
    Forbidden(String),
    /// Error raised by a custom [`RecordBatchConsumer`].
    ///
    /// [`RecordBatchConsumer`]: crate::core::engine::RecordBatchConsumer
    Consumer(Box<dyn std::error::Error + Send + Sync>),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::DuckDb(e) => e.fmt(f),
            EngineError::Arrow(e) => e.fmt(f),
            EngineError::Cancelled => f.write_str("query cancelled"),
            EngineError::Forbidden(reason) => f.write_str(reason),
            EngineError::Consumer(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for EngineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            EngineError::DuckDb(e) => Some(e),
            EngineError::Arrow(e) => Some(e),
            EngineError::Cancelled | EngineError::Forbidden(_) => None,
            EngineError::Consumer(e) => Some(e.as_ref()),
        }
    }
}

impl From<duckdb::Error> for EngineError {
    fn from(e: duckdb::Error) -> Self {
        EngineError::DuckDb(e)
    }
}

impl From<ArrowError> for EngineError {
    fn from(e: ArrowError) -> Self {
        EngineError::Arrow(e)
    }
}
