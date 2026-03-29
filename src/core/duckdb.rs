use crate::cli::options::UQ_ATTACHED_DB_NAME;
use crate::core::engine::{ExecutableQuery, RecordBatchConsumer, UQueryEngine};
use duckdb::Connection;
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use tokio::time::Instant;
use tracing::debug;

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
        if let Err(e) = conn.prepare(sql) {
            self.pool.release(conn);
            return Err(e.to_string());
        }
        Ok(Box::new(DuckDbQuery {
            conn: Some(conn),
            pool: Arc::clone(&self.pool),
            sql: sql.to_string(),
        }))
    }
}

struct DuckDbQuery {
    conn: Option<Connection>,
    pool: Arc<ConnectionPool>,
    sql: String,
}

impl Drop for DuckDbQuery {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            self.pool.release(conn);
        }
    }
}

impl ExecutableQuery for DuckDbQuery {
    fn execute(&mut self, consumer: &mut dyn RecordBatchConsumer) -> Result<(), String> {
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
}
