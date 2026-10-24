use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;
use std::sync::Arc;

pub trait RecordBatchConsumer: Send {
    fn on_schema(&mut self, schema: SchemaRef) -> Result<(), String>;
    fn on_batch(&mut self, batch: RecordBatch) -> Result<(), String>;
    fn finish(&mut self) -> Result<(), String>;
}

/// Stops a query from another thread.
pub trait QueryCanceller: Send + Sync {
    fn cancel(&self);
}

/// A validated, ready-to-stream query returned by [`UQueryEngine::prepare`].
pub trait ExecutableQuery: Send {
    fn execute(&mut self, consumer: &mut dyn RecordBatchConsumer) -> Result<(), String>;

    /// Return a handle that cancels this query. Once the query is dropped,
    /// cancelling is a no-op.
    fn canceller(&self) -> Arc<dyn QueryCanceller>;
}

pub trait UQueryEngine: Send + Sync {
    /// Validate `sql` and return an executable handle or an error if the query
    /// is invalid.
    fn prepare(&self, sql: &str) -> Result<Box<dyn ExecutableQuery>, String>;
}
