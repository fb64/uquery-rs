use arrow::datatypes::SchemaRef;
use arrow::record_batch::RecordBatch;

pub trait RecordBatchConsumer: Send {
    fn on_schema(&mut self, schema: SchemaRef) -> Result<(), String>;
    fn on_batch(&mut self, batch: RecordBatch) -> Result<(), String>;
    fn finish(&mut self) -> Result<(), String>;
}

/// A validated, ready-to-stream query returned by [`UQueryEngine::prepare`].
pub trait ExecutableQuery: Send {
    fn execute(&mut self, consumer: &mut dyn RecordBatchConsumer) -> Result<(), String>;
}

pub trait UQueryEngine: Send + Sync {
    /// Validate `sql` and return an executable handle or an error if the query
    /// is invalid.
    fn prepare(&self, sql: &str) -> Result<Box<dyn ExecutableQuery>, String>;
}
