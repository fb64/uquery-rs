use crate::cli::options::UQ_ATTACHED_DB_NAME;
use crate::core::engine::{ExecutableQuery, RecordBatchConsumer, UQueryEngine};
use duckdb::Connection;
use std::sync::Mutex;
use tokio::time::Instant;
use tracing::debug;

pub struct DuckDbEngine {
    connection: Mutex<Connection>,
    attached: bool,
}

impl DuckDbEngine {
    pub fn new(connection: Connection, attached: bool) -> Self {
        Self {
            connection: Mutex::new(connection),
            attached,
        }
    }

    fn get_connection(&self) -> Result<Connection, String> {
        let conn = self
            .connection
            .lock()
            .map_err(|_| "duckdb connection mutex poisoned".to_string())?
            .try_clone()
            .map_err(|e| e.to_string())?;
        if self.attached {
            conn.execute(format!("USE {UQ_ATTACHED_DB_NAME};").as_str(), [])
                .map_err(|e| e.to_string())?;
        }
        Ok(conn)
    }
}

impl UQueryEngine for DuckDbEngine {
    fn prepare(&self, sql: &str) -> Result<Box<dyn ExecutableQuery>, String> {
        let conn = self.get_connection()?;
        conn.prepare(sql).map_err(|e| e.to_string())?;
        Ok(Box::new(DuckDbQuery {
            conn,
            sql: sql.to_string(),
        }))
    }
}

struct DuckDbQuery {
    conn: Connection,
    sql: String,
}

impl ExecutableQuery for DuckDbQuery {
    fn execute(&mut self, consumer: &mut dyn RecordBatchConsumer) -> Result<(), String> {
        let start = Instant::now();
        let mut stmt = self.conn.prepare(&self.sql).map_err(|e| e.to_string())?;
        let arrow = stmt.query_arrow([]).map_err(|e| e.to_string())?;
        debug!("run: [{}] in {:?}", self.sql, start.elapsed());
        consumer.on_schema(arrow.get_schema())?;
        for batch in arrow {
            consumer.on_batch(batch)?;
        }
        consumer.finish()
    }
}
