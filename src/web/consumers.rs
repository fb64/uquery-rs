use crate::core::engine::RecordBatchConsumer;
use arrow::datatypes::SchemaRef;
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::{RecordBatch, RecordBatchWriter};
use std::io::Write;

/// Generic consumer that delegates `on_batch` and `finish` to any `RecordBatchWriter`.
/// `on_schema` is a no-op; writers that need the schema upfront (e.g., StreamWriter) should
/// wrap this type and initialize the inner writer lazily in their own `on_schema`.
pub(crate) struct WriterConsumer<W: RecordBatchWriter + Send> {
    writer: Option<W>,
}

impl<W: RecordBatchWriter + Send> WriterConsumer<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer: Some(writer),
        }
    }
}

impl<W: RecordBatchWriter + Send> RecordBatchConsumer for WriterConsumer<W> {
    fn on_schema(&mut self, _schema: SchemaRef) -> Result<(), String> {
        Ok(())
    }

    fn on_batch(&mut self, batch: RecordBatch) -> Result<(), String> {
        self.writer
            .as_mut()
            .unwrap()
            .write(&batch)
            .map_err(|e| e.to_string())
    }

    fn finish(&mut self) -> Result<(), String> {
        self.writer
            .take()
            .unwrap()
            .close()
            .map_err(|e| e.to_string())
    }
}

/// Arrow IPC consumer: `StreamWriter` requires the schema before the first batch,
/// so the sink is hold in an `Option` and the inner `WriterConsumer` is initialized lazily
/// in `on_schema`, then delegate `on_batch` and `finish` to it.
pub(crate) struct ArrowConsumer<W: Write + Send> {
    inner: WriterConsumer<StreamWriter<W>>,
    sink: Option<W>,
}

impl<W: Write + Send> ArrowConsumer<W> {
    pub fn new(sink: W) -> Self {
        Self {
            inner: WriterConsumer { writer: None },
            sink: Some(sink),
        }
    }
}

impl<W: Write + Send> RecordBatchConsumer for ArrowConsumer<W> {
    fn on_schema(&mut self, schema: SchemaRef) -> Result<(), String> {
        let sink = self.sink.take().unwrap();
        self.inner.writer = Some(StreamWriter::try_new(sink, &schema).map_err(|e| e.to_string())?);
        Ok(())
    }

    fn on_batch(&mut self, batch: RecordBatch) -> Result<(), String> {
        self.inner.on_batch(batch)
    }

    fn finish(&mut self) -> Result<(), String> {
        self.inner.finish()
    }
}
