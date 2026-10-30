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

impl DuckDbCanceller {
    fn is_cancelled(&self) -> bool {
        self.state.lock().unwrap().cancelled
    }
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
        if self.canceller.is_cancelled() {
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
        // The result is fully computed before the first batch, so an interrupt has
        // no effect while streaming it: check for cancellation between batches.
        for batch in arrow {
            if self.canceller.is_cancelled() {
                return Err(EngineError::Cancelled);
            }
            consumer.on_batch(batch)?;
        }
        consumer.finish()
    }

    fn canceller(&self) -> Arc<dyn QueryCanceller> {
        self.canceller.clone()
    }
}

/// Rejects anything but a single `SELECT` statement, and queries using
/// [`BLOCKED_FUNCTIONS`] or [`BLOCKED_VIEWS`]. `json_serialize_sql` uses DuckDB's own parser and only
/// serializes `SELECT` statements, so it classifies statements exactly as they will
/// be executed.
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
        Some(0 | 1) => {}
        _ => {
            return Err(EngineError::Forbidden(
                "only one statement per query is allowed".to_string(),
            ));
        }
    }
    match find_blocked(&serialized) {
        Some(name) => Err(EngineError::Forbidden(format!(
            "{name} is not available to API clients"
        ))),
        None => Ok(()),
    }
}

/// Functions not available in read-only mode, because they:
/// - reveal the server's settings, secrets, file paths or other clients' activity,
/// - run SQL held in a string, which [`find_blocked`] can't look into,
/// - change the server's state from a `SELECT`,
/// - take memory addresses as arguments (`arrow_scan`).
///
/// Catalog introspection (`duckdb_tables()`, `DESCRIBE`, ...) stays available.
const BLOCKED_FUNCTIONS: &[&str] = &[
    "arrow_scan",
    "arrow_scan_dumb",
    "checkpoint",
    "create_external_resource",
    "current_setting",
    "deregister_external_resource",
    "destroy_external_resource",
    "disable_logging",
    "disable_profiling",
    "duckdb_databases",
    "duckdb_extension_repositories",
    "duckdb_extensions",
    "duckdb_external_file_cache",
    "duckdb_external_resource_types",
    "duckdb_external_resources",
    "duckdb_log_contexts",
    "duckdb_logs",
    "duckdb_logs_parsed",
    "duckdb_prepared_statements",
    "duckdb_secrets",
    "duckdb_settings",
    "duckdb_temporary_files",
    "enable_logging",
    "enable_profiling",
    "force_checkpoint",
    "json_execute_serialized_sql",
    "query",
    "query_table",
    "register_external_resource",
    "register_external_resource_type",
    "truncate_duckdb_logs",
    "which_secret",
];

/// Built-in views revealing the same information as [`BLOCKED_FUNCTIONS`]. Kept apart
/// so that a client's table can be named like a blocked function, e.g. `query`.
const BLOCKED_VIEWS: &[&str] = &[
    "duckdb_databases",
    "duckdb_logs",
    "pg_prepared_statements",
    "pg_settings",
    "pragma_database_list",
];

/// Returns the first blocked function or view the parsed query uses, wherever it
/// appears (subquery, CTE, ...) and however it is qualified or cased.
fn find_blocked(node: &serde_json::Value) -> Option<String> {
    match node {
        serde_json::Value::Object(fields) => {
            let blocked = |field: &str, names: &[&str]| {
                fields
                    .get(field)
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_lowercase)
                    .filter(|name| names.contains(&name.as_str()))
            };
            blocked("function_name", BLOCKED_FUNCTIONS)
                .or_else(|| blocked("table_name", BLOCKED_VIEWS))
                .or_else(|| fields.values().find_map(find_blocked))
        }
        serde_json::Value::Array(items) => items.iter().find_map(find_blocked),
        _ => None,
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
    fn read_only_rejects_server_introspection_test() {
        let engine = make_engine();
        for sql in [
            "SELECT * FROM duckdb_settings()",
            "SELECT current_setting('allowed_directories')",
            "SELECT * FROM pg_catalog.pg_settings",
            "SELECT * FROM duckdb_secrets()",
            "SELECT * FROM which_secret('s3://bucket/file', 's3')",
            "SELECT path FROM duckdb_databases()",
            "SELECT * FROM pragma_database_list",
            "SELECT install_path FROM duckdb_extensions()",
            "SELECT * FROM duckdb_temporary_files()",
            "SELECT * FROM duckdb_external_file_cache()",
            "SELECT * FROM duckdb_logs()",
            "SELECT * FROM duckdb_prepared_statements()",
            // Hidden in a subquery or a CTE, qualified, or in another case.
            "SELECT 1 WHERE 'x' IN (SELECT name FROM duckdb_settings())",
            "WITH s AS (FROM duckdb_secrets()) FROM s",
            "FROM main.duckdb_settings()",
            "SELECT * FROM \"DuckDB_Settings\"()",
            "SELECT upper(CURRENT_SETTING('threads'))",
            "SUMMARIZE SELECT * FROM duckdb_settings()",
            // Run SQL held in a string, which the check can't look into.
            "SELECT * FROM query('SELECT * FROM duckdb_settings()')",
            "SELECT * FROM query_table('pg_settings')",
            "SELECT * FROM json_execute_serialized_sql(json_serialize_sql('SELECT 1'))",
            "SELECT * FROM pg_prepared_statements",
            "SELECT * FROM duckdb_extension_repositories()",
            // Change the server's state from a SELECT.
            "SELECT * FROM checkpoint()",
            "SELECT * FROM force_checkpoint()",
            "SELECT * FROM truncate_duckdb_logs()",
            "SELECT * FROM enable_logging()",
            "SELECT * FROM disable_logging()",
            "SELECT * FROM enable_profiling()",
            "SELECT * FROM disable_profiling()",
            "SELECT * FROM create_external_resource('x')",
            // Dereferences its arguments.
            "SELECT * FROM arrow_scan(1, 2, 3)",
        ] {
            let err = engine.query(sql).unwrap_err();
            assert!(matches!(err, EngineError::Forbidden(_)), "{sql}: {err}");
        }
    }

    #[test]
    fn read_only_allows_tables_named_like_blocked_functions_test() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE query(x INT); CREATE TABLE checkpoint(x INT);")
            .unwrap();
        let engine = DuckDbEngine::new(conn, false, 1).unwrap();
        for sql in ["SELECT * FROM query", "SELECT * FROM checkpoint"] {
            engine.query(sql).unwrap_or_else(|e| panic!("{sql}: {e}"));
        }
    }

    /// DuckDB updates add table functions that may reveal or change the server's
    /// state: this fails on each new one until it is reviewed and listed here or in
    /// [`BLOCKED_FUNCTIONS`].
    #[test]
    fn every_table_function_is_reviewed_test() {
        const ALLOWED_FUNCTIONS: &[&str] = &[
            "duckdb_approx_database_count",
            "duckdb_available_metrics",
            "duckdb_columns",
            "duckdb_connection_count",
            "duckdb_constraints",
            "duckdb_coordinate_systems",
            "duckdb_dependencies",
            "duckdb_dialects",
            "duckdb_eviction_queues",
            "duckdb_functions",
            "duckdb_grammar_extensions",
            "duckdb_indexes",
            "duckdb_keywords",
            "duckdb_memory",
            "duckdb_optimizers",
            "duckdb_profiling_settings",
            "duckdb_schemas",
            "duckdb_secret_type_parameters",
            "duckdb_secret_types",
            "duckdb_sequences",
            "duckdb_table_sample",
            "duckdb_tables",
            "duckdb_triggers",
            "duckdb_types",
            "duckdb_variables",
            "duckdb_views",
            "generate_series",
            "glob",
            "histogram",
            "histogram_values",
            "json_each",
            "json_tree",
            "parquet_bloom_probe",
            "parquet_file_metadata",
            "parquet_full_metadata",
            "parquet_kv_metadata",
            "parquet_metadata",
            "parquet_scan",
            "parquet_schema",
            "pragma_collations",
            "pragma_database_size",
            "pragma_metadata_info",
            "pragma_platform",
            "pragma_show",
            "pragma_storage_info",
            "pragma_table_info",
            "pragma_user_agent",
            "pragma_version",
            "range",
            "read_blob",
            "read_csv",
            "read_csv_auto",
            "read_duckdb",
            "read_json",
            "read_json_auto",
            "read_json_objects",
            "read_json_objects_auto",
            "read_ndjson",
            "read_ndjson_auto",
            "read_ndjson_objects",
            "read_parquet",
            "read_single_csv_file",
            "read_single_json_file",
            "read_single_parquet_file",
            "read_text",
            "repeat",
            "repeat_row",
            "seq_scan",
            "sniff_csv",
            "summary",
            "test_all_types",
            "test_vector_types",
            "unnest",
        ];
        let conn = Connection::open_in_memory().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT function_name FROM duckdb_functions() \
                 WHERE function_type IN ('table', 'table_macro') ORDER BY 1",
            )
            .unwrap();
        let unreviewed: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(Result::unwrap)
            .filter(|name: &String| {
                !ALLOWED_FUNCTIONS.contains(&name.as_str())
                    && !BLOCKED_FUNCTIONS.contains(&name.as_str())
            })
            .collect();
        assert!(unreviewed.is_empty(), "unreviewed: {unreviewed:#?}");
    }

    #[test]
    fn read_only_allows_catalog_introspection_test() {
        let engine = make_engine();
        for sql in [
            "SELECT * FROM duckdb_tables()",
            "SELECT * FROM duckdb_columns()",
            "SELECT * FROM information_schema.tables",
            "SHOW TABLES",
            "SELECT version()",
        ] {
            engine.query(sql).unwrap_or_else(|e| panic!("{sql}: {e}"));
        }
    }

    #[test]
    fn allow_writes_allows_server_introspection_test() {
        let engine = make_engine().allow_writes(true);
        engine
            .query("SELECT name, value FROM duckdb_settings()")
            .unwrap();
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

    struct CancellingConsumer(Arc<dyn QueryCanceller>, usize);

    impl RecordBatchConsumer for CancellingConsumer {
        fn on_schema(&mut self, _schema: SchemaRef) -> Result<(), EngineError> {
            Ok(())
        }

        fn on_batch(&mut self, _batch: RecordBatch) -> Result<(), EngineError> {
            self.1 += 1;
            self.0.cancel();
            Ok(())
        }

        fn finish(&mut self) -> Result<(), EngineError> {
            Ok(())
        }
    }

    #[test]
    fn cancel_while_streaming_test() {
        let engine = make_engine();
        let mut query = engine.prepare("SELECT * FROM range(1000000)").unwrap();
        let mut consumer = CancellingConsumer(query.canceller(), 0);
        let result = query.execute(&mut consumer);
        assert!(matches!(result, Err(EngineError::Cancelled)), "{result:?}");
        assert_eq!(consumer.1, 1);
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
