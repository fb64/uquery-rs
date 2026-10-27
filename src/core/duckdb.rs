use crate::core::engine::{ExecutableQuery, QueryCanceller, RecordBatchConsumer, UQueryEngine};
use duckdb::{Connection, InterruptHandle};
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use tokio::time::Instant;
use tracing::debug;

/// Name of the database attached by the caller and made the default schema
/// of every pooled connection when `DuckDbEngine::new` is called with `attached`.
pub const UQ_ATTACHED_DB_NAME: &str = "uquery_attached_db";

struct ConnectionPool {
    connections: Mutex<VecDeque<Connection>>,
    condvar: Condvar,
}

impl ConnectionPool {
    fn new(root: &Connection, size: usize, attached: bool) -> Result<Self, String> {
        let mut conns = VecDeque::with_capacity(size);
        for _ in 0..size {
            let conn = root.try_clone().map_err(|e| e.to_string())?;
            if attached {
                conn.execute(format!("USE {UQ_ATTACHED_DB_NAME};").as_str(), [])
                    .map_err(|e| e.to_string())?;
            }
            conns.push_back(conn);
        }
        Ok(Self {
            connections: Mutex::new(conns),
            condvar: Condvar::new(),
        })
    }

    fn acquire(&self) -> Connection {
        let mut guard = self.connections.lock().unwrap();
        loop {
            if let Some(conn) = guard.pop_front() {
                return conn;
            }
            guard = self.condvar.wait(guard).unwrap();
        }
    }

    fn release(&self, conn: Connection) {
        self.connections.lock().unwrap().push_back(conn);
        self.condvar.notify_one();
    }
}

pub struct DuckDbEngine {
    pool: Arc<ConnectionPool>,
}

impl DuckDbEngine {
    pub fn new(connection: Connection, attached: bool, pool_size: usize) -> Result<Self, String> {
        Ok(Self {
            pool: Arc::new(ConnectionPool::new(&connection, pool_size, attached)?),
        })
    }
}

impl UQueryEngine for DuckDbEngine {
    fn prepare(&self, sql: &str) -> Result<Box<dyn ExecutableQuery>, String> {
        let conn = self.pool.acquire();
        let canceller = Arc::new(DuckDbCanceller {
            state: Mutex::new(CancelState {
                interrupt: Some(conn.interrupt_handle()),
                cancelled: false,
            }),
        });
        Ok(Box::new(DuckDbQuery {
            conn: Some(conn),
            pool: Arc::clone(&self.pool),
            sql: sql.to_string(),
            canceller,
        }))
    }
}

struct CancelState {
    // None once the query released its connection, so a late cancel cannot
    // interrupt the next query running on the pooled connection.
    interrupt: Option<Arc<InterruptHandle>>,
    cancelled: bool,
}

struct DuckDbCanceller {
    state: Mutex<CancelState>,
}

impl QueryCanceller for DuckDbCanceller {
    fn cancel(&self) {
        let mut state = self.state.lock().unwrap();
        state.cancelled = true;
        if let Some(interrupt) = &state.interrupt {
            interrupt.interrupt();
        }
    }
}

struct DuckDbQuery {
    conn: Option<Connection>,
    pool: Arc<ConnectionPool>,
    sql: String,
    canceller: Arc<DuckDbCanceller>,
}

impl Drop for DuckDbQuery {
    fn drop(&mut self) {
        self.canceller.state.lock().unwrap().interrupt = None;
        if let Some(conn) = self.conn.take() {
            self.pool.release(conn);
        }
    }
}

impl ExecutableQuery for DuckDbQuery {
    fn execute(&mut self, consumer: &mut dyn RecordBatchConsumer) -> Result<(), String> {
        // DuckDB ignores an interrupt received while the connection is idle, so a
        // cancel that lands before the query starts must be checked here.
        if self.canceller.state.lock().unwrap().cancelled {
            return Err("query cancelled".to_string());
        }
        let conn = self.conn.as_ref().expect("connection already consumed");
        let start = Instant::now();
        let mut stmt = conn.prepare(&self.sql).map_err(|e| e.to_string())?;
        let arrow = stmt.query_arrow([]).map_err(|e| e.to_string())?;
        debug!("run: [{}] in {:?}", self.sql, start.elapsed());
        consumer.on_schema(arrow.get_schema())?;
        for batch in arrow {
            consumer.on_batch(batch)?;
        }
        consumer.finish()
    }

    fn canceller(&self) -> Arc<dyn QueryCanceller> {
        self.canceller.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::datatypes::SchemaRef;
    use arrow::record_batch::RecordBatch;
    use std::time::Duration;

    struct CountingConsumer(usize);

    impl RecordBatchConsumer for CountingConsumer {
        fn on_schema(&mut self, _schema: SchemaRef) -> Result<(), String> {
            Ok(())
        }

        fn on_batch(&mut self, batch: RecordBatch) -> Result<(), String> {
            self.0 += batch.num_rows();
            Ok(())
        }

        fn finish(&mut self) -> Result<(), String> {
            Ok(())
        }
    }

    const ENDLESS_QUERY: &str = "SELECT count(*) FROM range(1000000000000)";

    fn make_engine() -> DuckDbEngine {
        DuckDbEngine::new(Connection::open_in_memory().unwrap(), false, 1).unwrap()
    }

    #[test]
    fn cancel_running_query_test() {
        let engine = make_engine();
        let mut query = engine.prepare(ENDLESS_QUERY).unwrap();
        let canceller = query.canceller();
        let start = Instant::now();
        let cancel_thread = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            canceller.cancel();
        });
        let result = query.execute(&mut CountingConsumer(0));
        cancel_thread.join().unwrap();
        assert!(result.unwrap_err().contains("Interrupted"));
        assert!(start.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn cancel_before_execute_test() {
        let engine = make_engine();
        let mut query = engine.prepare("SELECT 1").unwrap();
        query.canceller().cancel();
        assert_eq!(
            query.execute(&mut CountingConsumer(0)),
            Err("query cancelled".to_string())
        );
    }

    #[test]
    fn query_collects_result_test() {
        let result = make_engine()
            .query("SELECT * FROM range(3) t(n) WHERE n > 5")
            .unwrap();
        assert_eq!(result.num_rows(), 0);
        assert_eq!(result.schema.unwrap().field(0).name(), "n");

        let result = make_engine().query("SELECT * FROM range(5000)").unwrap();
        assert_eq!(result.num_rows(), 5000);
    }

    #[test]
    fn cancel_after_drop_does_not_affect_next_query_test() {
        let engine = make_engine();
        let canceller = engine.prepare("SELECT 1").unwrap().canceller();
        // The single pooled connection is now used by a new query.
        let mut query = engine.prepare("SELECT 42").unwrap();
        canceller.cancel();
        let mut consumer = CountingConsumer(0);
        query.execute(&mut consumer).unwrap();
        assert_eq!(consumer.0, 1);
    }
}
