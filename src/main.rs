use std::fmt::Display;
use std::sync::{Arc, Mutex};

use arrow::array::RecordBatchWriter;
use arrow::csv::Writer;
use arrow::ipc::writer::StreamWriter;
use arrow::json::ArrayWriter;
use axum::{Json, Router};
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::http::header::{ACCEPT, CONTENT_TYPE};
use axum::response::Response;
use axum::routing::post;
use duckdb::{Arrow, Connection};
use serde::{Deserialize, Serialize};
use tokio::task::spawn_blocking;
use tokio::time::Instant;
use tokio_util::io::{ReaderStream, SyncIoBridge};
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tracing::{debug, info};
use crate::cli::UQ_ATTACHED_DB_NAME;

mod cli;

const CONTENT_TYPE_CSV: &str = "text/csv";
const CONTENT_TYPE_JSON: &str = "application/json";
const CONTENT_TYPE_ARROW: &str = "application/vnd.apache.arrow.stream";

#[derive(Deserialize, Serialize)]
struct QueryRequest {
    query: String,
}

enum QueryResponseFormat {
    CSV,
    JSON,
    ARROW,
}

impl Display for QueryResponseFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            QueryResponseFormat::CSV => CONTENT_TYPE_CSV.to_string(),
            QueryResponseFormat::JSON => CONTENT_TYPE_JSON.to_string(),
            QueryResponseFormat::ARROW => CONTENT_TYPE_ARROW.to_string()
        };
        write!(f, "{}", str)
    }
}


struct UQueryState {
    duckdb_connection: Mutex<Connection>,
    attached: bool,
}

impl UQueryState {
    fn get_new_connection(&self) -> Connection {
        let new_conn = self.duckdb_connection.try_lock().unwrap().try_clone().unwrap();
        if self.attached{

            new_conn.execute(format!("USE {UQ_ATTACHED_DB_NAME};").as_str(),[]).unwrap();
        }
        new_conn
    }
}

#[tokio::main]
async fn main() {
    let cli_options = cli::parse();
    let start = Instant::now();
    let addr = format!("{}:{}", cli_options.addr, cli_options.port);
    let conn = Connection::open_in_memory().unwrap();
    for init_query in cli_options.init_script(){
        conn.execute(init_query.as_str(), []).unwrap();
    }
    let state = Arc::new(UQueryState { duckdb_connection: Mutex::new(conn), attached:cli_options.db_file.is_some() });
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    info!("uQuery server started in {:?}",start.elapsed());
    debug!("listening on {}",addr);
    axum::serve(listener, app(state)).await.unwrap();
}

fn app(state: Arc<UQueryState>) -> Router {
    Router::new().route("/", post(query)).with_state(state)
        .layer(ServiceBuilder::new().layer(CompressionLayer::new()))
}

async fn query(State(state): State<Arc<UQueryState>>, headers: HeaderMap, Json(payload): Json<QueryRequest>) -> Result<Response, StatusCode> {
    let format = match headers.get(ACCEPT).unwrap().to_str().unwrap().to_lowercase().as_str() {
        CONTENT_TYPE_JSON => { Ok(QueryResponseFormat::JSON) }
        CONTENT_TYPE_CSV => { Ok(QueryResponseFormat::CSV) }
        CONTENT_TYPE_ARROW => { Ok(QueryResponseFormat::ARROW) }
        _ => {
            Err(StatusCode::NOT_ACCEPTABLE)
        }
    }?;

    let content_type = format.to_string();

    let (tx, rx) = tokio::io::duplex(65_536);
    let reader_stream = ReaderStream::new(rx);
    spawn_blocking(move || {
        let bridge = SyncIoBridge::new(tx);
        let query_start = Instant::now();
        let conn = state.get_new_connection();
        //conn.execute("USE uquery;", []).unwrap();
        let mut statement = conn.prepare(payload.query.as_str()).unwrap();
        let arrow = statement.query_arrow([]).unwrap();
        debug!("run: [{}] in {:?}",payload.query, query_start.elapsed());
        match format {
            QueryResponseFormat::CSV => {
                let writer = Writer::new(bridge);
                handle_response_write(writer, arrow);
            }
            QueryResponseFormat::JSON => {
                let writer = ArrayWriter::new(bridge);
                handle_response_write(writer, arrow);
            }
            QueryResponseFormat::ARROW => {
                let writer = StreamWriter::try_new(bridge, &*arrow.get_schema()).unwrap();
                handle_response_write(writer, arrow);
            }
        };
    });

    Ok(axum::response::Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, content_type)
        .body(Body::from_stream(reader_stream))
        .unwrap())
}

fn handle_response_write<W: RecordBatchWriter>(mut writer: W, data: Arrow) {
    for rb in data {
        writer.write(&rb).unwrap();
    }
    writer.close().unwrap();
}


#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::str::from_utf8;
    use std::sync::{Arc, Mutex};

    use axum::body::Body;
    use axum::http;
    use axum::http::{Request, StatusCode};
    use axum::http::header::{ACCEPT, ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_TYPE};
    use axum::response::Response;
    use duckdb::Connection;
    use futures_util::StreamExt;
    use polars::error::PolarsError;
    use polars_io::ipc::IpcStreamReader;
    use polars_io::SerReader;
    use serde_json::Value;
    use tower::ServiceExt;

    use crate::{app, QueryRequest, QueryResponseFormat, UQueryState};
    use crate::cli::UQ_ATTACHED_DB_NAME;

    const TEST_QUERY: &str = "SELECT * FROM (VALUES (1,'Rust','Safe, concurrent, performant systems language')) Language(Id,Name,Description)";
    const TEST_QUERY_ATTACHED: &str = "SELECT * from language order by id";

    #[tokio::test]
    async fn query_json_test() {
        let response = perform_request(QueryRequest { query: TEST_QUERY.to_string() }, QueryResponseFormat::JSON).await;
        assert_eq!(response.status(), StatusCode::OK);
        let result = read_response(response).await;
        assert_eq!(from_utf8(&*result).unwrap(), "[{\"Id\":1,\"Name\":\"Rust\",\"Description\":\"Safe, concurrent, performant systems language\"}]");
    }

    #[tokio::test]
    async fn query_csv_test() {
        let response = perform_request(QueryRequest { query: TEST_QUERY.to_string() }, QueryResponseFormat::CSV).await;
        assert_eq!(response.status(), StatusCode::OK);
        let result = read_response(response).await;
        assert_eq!(from_utf8(&*result).unwrap(), "Id,Name,Description\n1,Rust,\"Safe, concurrent, performant systems language\"\n");
    }

    #[tokio::test]
    async fn query_arrow_test() -> Result<(), PolarsError> {
        let response = perform_request(
            QueryRequest { query: TEST_QUERY.to_string() },
            QueryResponseFormat::ARROW,
        ).await;
        assert_eq!(response.status(), StatusCode::OK);
        let result = read_response(response).await;
        let df = IpcStreamReader::new(Cursor::new(result)).finish()?;
        let id = df.column("Id")?.i32()?.get(0).unwrap();
        let name = df.column("Name")?.str()?.get(0).unwrap();
        let description = df.column("Description")?.str()?.get(0).unwrap();
        assert_eq!(id, 1);
        assert_eq!(name, "Rust");
        assert_eq!(description, "Safe, concurrent, performant systems language");
        Ok(())


        //assert_eq!(from_utf8(&*result).unwrap(),"{\"Id\":1,\"Name\":\"Rust\",\"Description\":\"Safe, concurrent, performant systems language\"}\n");
    }


    #[tokio::test]
    async fn query_json_gzip_test() {
        let response = perform_request_compress(
            QueryRequest { query: TEST_QUERY.to_string() },
            QueryResponseFormat::JSON,
            true,
        ).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers().get(CONTENT_ENCODING).unwrap(), "gzip");
        let result = read_response(response).await;
        assert_eq!(result[0], 0x1fu8);
        assert_eq!(result[1], 0x8bu8);
    }

    #[tokio::test]
    async fn query_attached_db_test() {
        let request = QueryRequest { query: TEST_QUERY_ATTACHED.to_string() };
        let json = serde_json::to_string(&request).unwrap();

        let builder = Request::builder()
            .method(http::Method::POST)
            .uri("/")
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, QueryResponseFormat::JSON.to_string());

        let conn = Connection::open_in_memory().unwrap();
        conn.execute(format!("ATTACH 'tests/test.db' as {UQ_ATTACHED_DB_NAME};").as_str(), []).unwrap();
        let state = Arc::new(UQueryState { duckdb_connection: Mutex::new(conn), attached: true });
        let response = app(state).oneshot(
            builder.body(Body::from(json)).unwrap()
        ).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let result = read_response(response).await;
        let response_string = from_utf8(&*result).unwrap();
        let json_array:Vec<Value> = serde_json::from_str(response_string).unwrap();
        assert_eq!(json_array.len(),10);
        assert_eq!(json_array[0].get("id").unwrap().as_i64().unwrap(),1);
        assert_eq!(json_array[0].get("name").unwrap().as_str().unwrap(),"Rust");
    }

    async fn perform_request(request: QueryRequest, format: QueryResponseFormat) -> Response {
        perform_request_compress(request, format, false).await
    }

    async fn perform_request_compress(request: QueryRequest, format: QueryResponseFormat, compress: bool) -> Response {
        let json = serde_json::to_string(&request).unwrap();

        let mut builder = Request::builder()
            .method(http::Method::POST)
            .uri("/")
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, format.to_string());
        if compress {
            builder = builder.header(ACCEPT_ENCODING, "gzip");
        }
        let conn = Connection::open_in_memory().unwrap();
        let state = Arc::new(UQueryState { duckdb_connection: Mutex::new(conn), attached: false });
        app(state).oneshot(
            builder.body(Body::from(json)).unwrap()
        ).await.unwrap()
    }

    async fn read_response(response: Response) -> Vec<u8> {
        let mut stream = response.into_body().into_data_stream();
        let mut result = Vec::new();
        while let Some(item) = stream.next().await {
            match item {
                Ok(bytes) => {
                    for b in bytes {
                        result.push(b)
                    }
                }
                Err(e) => eprintln!("Error: {}", e),
            };
        }
        result
    }
}
