use crate::core::error::EngineError;
use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;
use std::sync::Arc;

pub trait RecordBatchConsumer: Send {
    fn on_schema(&mut self, schema: SchemaRef) -> Result<(), EngineError>;
    fn on_batch(&mut self, batch: RecordBatch) -> Result<(), EngineError>;
    fn finish(&mut self) -> Result<(), EngineError>;
}

/// Stops a query from another thread.
pub trait QueryCanceller: Send + Sync {
    fn cancel(&self);
}

/// A validated, ready-to-stream query returned by [`UQueryEngine::prepare`].
pub trait ExecutableQuery: Send {
    fn execute(&mut self, consumer: &mut dyn RecordBatchConsumer) -> Result<(), EngineError>;

    /// Return a handle that cancels this query. Once the query is dropped,
    /// cancelling is a no-op.
    fn canceller(&self) -> Arc<dyn QueryCanceller>;
}

pub trait UQueryEngine: Send + Sync {
    /// Validate `sql` and return an executable handle or an error if the query
    /// is invalid.
    fn prepare(&self, sql: &str) -> Result<Box<dyn ExecutableQuery>, EngineError>;

    /// Run `sql` and collect the whole result in memory. Use [`prepare`](Self::prepare)
    /// with a [`RecordBatchConsumer`] to stream large results instead.
    fn query(&self, sql: &str) -> Result<QueryResult, EngineError> {
        let mut result = QueryResult::default();
        self.prepare(sql)?.execute(&mut result)?;
        Ok(result)
    }
}

/// Result of [`UQueryEngine::query`]. `schema` is set even when no batch was
/// produced.
#[derive(Debug, Default)]
pub struct QueryResult {
    pub schema: Option<SchemaRef>,
    pub batches: Vec<RecordBatch>,
}

impl QueryResult {
    pub fn num_rows(&self) -> usize {
        self.batches.iter().map(RecordBatch::num_rows).sum()
    }
}

impl RecordBatchConsumer for QueryResult {
    fn on_schema(&mut self, schema: SchemaRef) -> Result<(), EngineError> {
        self.schema = Some(schema);
        Ok(())
    }

    fn on_batch(&mut self, batch: RecordBatch) -> Result<(), EngineError> {
        self.batches.push(batch);
        Ok(())
    }

    fn finish(&mut self) -> Result<(), EngineError> {
        Ok(())
    }
}
