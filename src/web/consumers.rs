use crate::core::engine::RecordBatchConsumer;
use crate::core::error::EngineError;
use arrow::datatypes::SchemaRef;
use arrow::ipc::CompressionType;
use arrow::ipc::writer::{IpcWriteOptions, StreamWriter};
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
    fn on_schema(&mut self, _schema: SchemaRef) -> Result<(), EngineError> {
        Ok(())
    }

    fn on_batch(&mut self, batch: RecordBatch) -> Result<(), EngineError> {
        self.writer
            .as_mut()
            .unwrap()
            .write(&batch)
            .map_err(EngineError::from)
    }

    fn finish(&mut self) -> Result<(), EngineError> {
        self.writer
            .take()
            .unwrap()
            .close()
            .map_err(EngineError::from)
    }
}

/// Arrow IPC consumer: `StreamWriter` requires the schema before the first batch,
/// so the sink is hold in an `Option` and the inner `WriterConsumer` is initialized lazily
/// in `on_schema`, then delegate `on_batch` and `finish` to it.
pub(crate) struct ArrowConsumer<W: Write + Send> {
    inner: WriterConsumer<StreamWriter<W>>,
    sink: Option<W>,
    compression: Option<CompressionType>,
}

impl<W: Write + Send> ArrowConsumer<W> {
    pub fn new(sink: W, compression: Option<CompressionType>) -> Self {
        Self {
            inner: WriterConsumer { writer: None },
            sink: Some(sink),
            compression,
        }
    }
}

impl<W: Write + Send> RecordBatchConsumer for ArrowConsumer<W> {
    fn on_schema(&mut self, schema: SchemaRef) -> Result<(), EngineError> {
        let sink = self.sink.take().unwrap();
        let write_options = IpcWriteOptions::default().try_with_compression(self.compression)?;
        self.inner.writer = Some(StreamWriter::try_new_with_options(
            sink,
            &schema,
            write_options,
        )?);
        Ok(())
    }

    fn on_batch(&mut self, batch: RecordBatch) -> Result<(), EngineError> {
        self.inner.on_batch(batch)
    }

    fn finish(&mut self) -> Result<(), EngineError> {
        self.inner.finish()
    }
}
