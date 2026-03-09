use crate::cli::options::UQ_ATTACHED_DB_NAME;
use duckdb::Connection;
use std::sync::Mutex;

pub(crate) struct UQueryState {
    duckdb_connection: Mutex<Connection>,
    attached: bool,
}

impl UQueryState {
    pub fn new(duckdb_connection: Connection, attached: bool) -> Self {
        Self {
            duckdb_connection: Mutex::new(duckdb_connection),
            attached,
        }
    }

    pub(crate) fn get_new_connection(&self) -> Connection {
        let new_conn = self
            .duckdb_connection
            .try_lock()
            .unwrap()
            .try_clone()
            .unwrap();
        if self.attached {
            new_conn
                .execute(format!("USE {UQ_ATTACHED_DB_NAME};").as_str(), [])
                .unwrap();
        }
        new_conn
    }
}
