use crate::core::engine::RecordBatchConsumer;
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
    compress: bool,
}

impl<W: Write + Send> ArrowConsumer<W> {
    pub fn new(sink: W, compress: bool) -> Self {
        Self {
            inner: WriterConsumer { writer: None },
            sink: Some(sink),
            compress,
        }
    }
}

impl<W: Write + Send> RecordBatchConsumer for ArrowConsumer<W> {
    fn on_schema(&mut self, schema: SchemaRef) -> Result<(), String> {
        let sink = self.sink.take().unwrap();
        // Arrow IPC responses use zstd (level 3, the codec's default) as their own
        // compression instead of relying on the outer HTTP gzip layer.
        let write_options = if self.compress {
            IpcWriteOptions::default()
                .try_with_compression(Some(CompressionType::ZSTD))
                .map_err(|e| e.to_string())?
        } else {
            IpcWriteOptions::default()
        };
        self.inner.writer = Some(
            StreamWriter::try_new_with_options(sink, &schema, write_options)
                .map_err(|e| e.to_string())?,
        );
        Ok(())
    }

    fn on_batch(&mut self, batch: RecordBatch) -> Result<(), String> {
        self.inner.on_batch(batch)
    }

    fn finish(&mut self) -> Result<(), String> {
        self.inner.finish()
    }
}
