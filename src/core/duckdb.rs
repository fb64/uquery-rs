use crate::core::engine::{ExecutableQuery, QueryCanceller, RecordBatchConsumer, UQueryEngine};
use crate::core::error::EngineError;
use duckdb::{Connection, InterruptHandle};
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use tokio::time::Instant;
use tracing::{debug, warn};

/// Name of the database attached by the caller and made the default schema
/// of every pooled connection when `DuckDbEngine::new` is called with `attached`.
pub const UQ_ATTACHED_DB_NAME: &str = "uquery_attached_db";

struct ConnectionPool {
    connections: Mutex<VecDeque<Connection>>,
    condvar: Condvar,
    // `USE` statement restoring the default database of a released connection.
    reset_sql: String,
}

impl ConnectionPool {
    fn new(root: &Connection, size: usize, attached: bool) -> Result<Self, EngineError> {
        let default_db: String = if attached {
            UQ_ATTACHED_DB_NAME.to_string()
        } else {
            root.query_row("SELECT current_database()", [], |row| row.get(0))?
        };
        let mut conns = VecDeque::with_capacity(size);
        for _ in 0..size {
            let conn = root.try_clone()?;
            if attached {
                conn.execute(format!("USE {UQ_ATTACHED_DB_NAME};").as_str(), [])?;
            }
            conns.push_back(conn);
        }
        Ok(Self {
            connections: Mutex::new(conns),
            condvar: Condvar::new(),
            reset_sql: format!("USE \"{}\";", default_db.replace('"', "\"\"")),
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
        // A query may have switched database with `USE`: restore the default so
        // the next client of this connection is not affected.
        if let Err(e) = conn.execute(&self.reset_sql, []) {
            warn!("failed to reset pooled connection: {e}");
        }
        self.connections.lock().unwrap().push_back(conn);
        self.condvar.notify_one();
    }
}

pub struct DuckDbEngine {
    pool: Arc<ConnectionPool>,
    writes_allowed: bool,
}

impl DuckDbEngine {
    pub fn new(
        connection: Connection,
        attached: bool,
        pool_size: usize,
    ) -> Result<Self, EngineError> {
        Ok(Self {
            pool: Arc::new(ConnectionPool::new(&connection, pool_size, attached)?),
            writes_allowed: false,
        })
    }

    /// Allow statements other than `SELECT` (`COPY`, `CREATE`, `ATTACH`, `USE`, ...).
    ///
    /// Off by default: all clients share the same database, so these statements let a
    /// client write files in the allowed directories or change what other clients see.
    pub fn allow_writes(mut self, allowed: bool) -> Self {
        self.writes_allowed = allowed;
        self
    }
}

impl UQueryEngine for DuckDbEngine {
    fn prepare(&self, sql: &str) -> Result<Box<dyn ExecutableQuery>, EngineError> {
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
            writes_allowed: self.writes_allowed,
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
    writes_allowed: bool,
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
    fn execute(&mut self, consumer: &mut dyn RecordBatchConsumer) -> Result<(), EngineError> {
        // DuckDB ignores an interrupt received while the connection is idle, so a
        // cancel that lands before the query starts must be checked here.
        if self.canceller.state.lock().unwrap().cancelled {
            return Err(EngineError::Cancelled);
        }
        let conn = self.conn.as_ref().expect("connection already consumed");
        if !self.writes_allowed {
            check_read_only(conn, &self.sql)?;
        }
        let start = Instant::now();
        let mut stmt = conn.prepare(&self.sql)?;
        let arrow = stmt.query_arrow([])?;
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

/// Rejects anything but a single `SELECT` statement. `json_serialize_sql` uses
/// DuckDB's own parser and only serializes `SELECT` statements, so it classifies
/// statements exactly as they will be executed.
fn check_read_only(conn: &Connection, sql: &str) -> Result<(), EngineError> {
    let serialized: String =
        conn.query_row("SELECT json_serialize_sql(?::VARCHAR)", [sql], |row| {
            row.get(0)
        })?;
    // Fail closed if the output format ever changes.
    let serialized: serde_json::Value = serde_json::from_str(&serialized)
        .map_err(|_| EngineError::Forbidden("query could not be classified".to_string()))?;
    if serialized["error"] == true {
        // Syntax errors are left to the real prepare, which reports them.
        if serialized["error_type"] == "parser" {
            return Ok(());
        }
        return Err(EngineError::Forbidden(
            "only SELECT queries are allowed".to_string(),
        ));
    }
    match serialized["statements"].as_array().map(Vec::len) {
        Some(0 | 1) => Ok(()),
        _ => Err(EngineError::Forbidden(
            "only one statement per query is allowed".to_string(),
        )),
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
        fn on_schema(&mut self, _schema: SchemaRef) -> Result<(), EngineError> {
            Ok(())
        }

        fn on_batch(&mut self, batch: RecordBatch) -> Result<(), EngineError> {
            self.0 += batch.num_rows();
            Ok(())
        }

        fn finish(&mut self) -> Result<(), EngineError> {
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
        assert!(result.unwrap_err().to_string().contains("Interrupted"));
        assert!(start.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn cancel_before_execute_test() {
        let engine = make_engine();
        let mut query = engine.prepare("SELECT 1").unwrap();
        query.canceller().cancel();
        assert!(matches!(
            query.execute(&mut CountingConsumer(0)),
            Err(EngineError::Cancelled)
        ));
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

    fn query_value(engine: &DuckDbEngine, sql: &str) -> String {
        let result = engine.query(sql).unwrap();
        arrow::util::display::array_value_to_string(result.batches[0].column(0), 0).unwrap()
    }

    #[test]
    fn read_only_rejects_writes_test() {
        let engine = make_engine();
        for sql in [
            "COPY (SELECT 1) TO 'uquery_test_never_written.csv'",
            "CREATE TABLE t AS SELECT 1",
            "CREATE VIEW \"data.csv\" AS SELECT 1",
            "INSERT INTO t VALUES (1)",
            "ATTACH ':memory:' AS other",
            "DETACH other",
            "USE memory",
            "SET threads = 1",
            "PRAGMA version",
            "CALL pragma_version()",
            "EXPORT DATABASE 'uquery_test_never_written'",
            "SELECT 1; SELECT 2",
            "SELECT 1; COPY (SELECT 1) TO 'uquery_test_never_written.csv'",
        ] {
            let err = engine.query(sql).unwrap_err();
            assert!(matches!(err, EngineError::Forbidden(_)), "{sql}: {err}");
        }
        assert!(!std::path::Path::new("uquery_test_never_written.csv").exists());
    }

    #[test]
    fn read_only_allows_queries_test() {
        let engine = make_engine();
        for sql in [
            "SELECT 42",
            "SELECT 42;",
            "FROM range(3)",
            "WITH t AS (SELECT 1 AS a) SELECT * FROM t",
            "VALUES (1), (2)",
            "DESCRIBE SELECT 1 AS a",
            "SUMMARIZE SELECT 1 AS a",
        ] {
            engine.query(sql).unwrap_or_else(|e| panic!("{sql}: {e}"));
        }
    }

    #[test]
    fn read_only_keeps_syntax_errors_test() {
        let err = make_engine().query("SELEC 1").unwrap_err();
        assert!(matches!(err, EngineError::DuckDb(_)), "{err}");
        assert!(err.to_string().contains("syntax error"), "{err}");
    }

    #[test]
    fn allow_writes_test() {
        let engine = make_engine().allow_writes(true);
        engine.query("CREATE TABLE t AS SELECT 7 AS v").unwrap();
        assert_eq!(query_value(&engine, "SELECT v FROM t"), "7");
    }

    #[test]
    fn released_connection_is_reset_test() {
        // Pool of one connection: every query reuses it.
        let engine = make_engine().allow_writes(true);
        engine.query("ATTACH ':memory:' AS other").unwrap();
        engine.query("USE other").unwrap();
        assert_eq!(query_value(&engine, "SELECT current_database()"), "memory");
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
