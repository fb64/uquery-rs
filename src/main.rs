use std::fmt::Display;
use axum::{Json, Router};
use axum::body::Body;
use axum::http::{HeaderMap, StatusCode};
use axum::http::header::{ACCEPT, CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use duckdb::Connection;
use polars::prelude::DataFrame;
use polars_core::utils::accumulate_dataframes_vertical_unchecked;
use polars_io::csv::CsvWriter;
use polars_io::ipc::IpcStreamWriter;
use polars_io::json::JsonWriter;
use polars_io::SerWriter;
use serde::{Deserialize, Serialize};
use tokio::task::spawn_blocking;
use tokio::time::Instant;
use tokio_util::io::{ReaderStream, SyncIoBridge};
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tracing::info;

const CONTENT_TYPE_CSV:&str = "text/csv";
const CONTENT_TYPE_JSON:&str = "application/json";
const CONTENT_TYPE_ARROW:&str = "application/vnd.apache.arrow.stream";

#[derive(Deserialize,Serialize)]
struct QueryRequest{
    query: String
}

enum QueryResponseFormat{
    CSV,
    JSON,
    ARROW
}

impl Display for QueryResponseFormat{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            QueryResponseFormat::CSV => CONTENT_TYPE_CSV.to_string(),
            QueryResponseFormat::JSON => CONTENT_TYPE_JSON.to_string(),
            QueryResponseFormat::ARROW => CONTENT_TYPE_ARROW.to_string()
        };
        write!(f, "{}", str)
    }
}

struct QueryResponse{
    data: DataFrame,
    format: QueryResponseFormat
}

impl IntoResponse for QueryResponse{
    fn into_response(mut self) -> Response {
        let content_type = self.format.to_string();
        let (tx, rx) = tokio::io::duplex(65_536);
        let reader_stream = ReaderStream::new(rx);
        spawn_blocking(move || {
            let bridge = SyncIoBridge::new(tx);
            match self.format {
                QueryResponseFormat::CSV => {CsvWriter::new(bridge).finish(&mut self.data).unwrap()}
                QueryResponseFormat::JSON => {JsonWriter::new(bridge).finish(&mut self.data).unwrap()}
                QueryResponseFormat::ARROW => {IpcStreamWriter::new(bridge).finish(&mut self.data).unwrap()}
            }
        });

        axum::response::Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, content_type)
            .body(Body::from_stream(reader_stream))
            .unwrap()
    }
}

#[tokio::main]
async fn main() {
    let start = Instant::now();
    tracing_subscriber::fmt::init();
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    info!("uQuery server started in {:?}",start.elapsed());
    axum::serve(listener, app()).await.unwrap();
}

fn app() -> Router{
    Router::new().route("/", post(query))
        .layer(ServiceBuilder::new().layer(CompressionLayer::new()))
}

async fn query(headers:HeaderMap, Json(payload): Json<QueryRequest>) -> Result<QueryResponse,StatusCode>{
    let response = handle_query(payload.query);

    match response {
        Ok(df) => {
            match headers.get(ACCEPT).unwrap().to_str().unwrap().to_lowercase().as_str() {
                CONTENT_TYPE_JSON => {Ok(QueryResponse{data:df,format:QueryResponseFormat::JSON})}
                CONTENT_TYPE_CSV => {Ok(QueryResponse{data:df,format:QueryResponseFormat::CSV})}
                CONTENT_TYPE_ARROW => {Ok(QueryResponse{data:df,format:QueryResponseFormat::ARROW})}
                _ => {
                    Err(StatusCode::NOT_ACCEPTABLE)
                }
            }
        }
        Err(_) => {Err(StatusCode::BAD_REQUEST)}
    }

}

fn handle_query(sql:String) -> Result<DataFrame,duckdb::Error>{
    let conn = Connection::open_in_memory()?;
    let mut stm = conn.prepare(sql.as_str())?;
    let pl = stm.query_polars([])?;
    let df = accumulate_dataframes_vertical_unchecked(pl);
    Ok(df)
}


#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::str::from_utf8;
    use axum::body::Body;
    use axum::http;
    use axum::http::{Request, StatusCode};
    use axum::http::header::{ACCEPT, ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_TYPE};
    use axum::response::Response;
    use tower::ServiceExt;
    use crate::{app, QueryRequest, QueryResponseFormat};
    use futures_util::StreamExt;
    use polars_core::error::PolarsError;
    use polars_io::ipc::IpcStreamReader;
    use polars_io::SerReader;

    const TEST_QUERY:&str = "SELECT * FROM (VALUES (1,'Rust','Safe, concurrent, performant systems language')) Language(Id,Name,Description)";

    #[tokio::test]
    async fn query_json_test() {
        let response = perform_request(QueryRequest{query: TEST_QUERY.to_string()},QueryResponseFormat::JSON).await;
        assert_eq!(response.status(), StatusCode::OK);
        let result = read_response(response).await;
        assert_eq!(from_utf8(&*result).unwrap(),"{\"Id\":1,\"Name\":\"Rust\",\"Description\":\"Safe, concurrent, performant systems language\"}\n");
    }

    #[tokio::test]
    async fn query_csv_test() {
        let response = perform_request(QueryRequest{query: TEST_QUERY.to_string()},QueryResponseFormat::CSV).await;
        assert_eq!(response.status(), StatusCode::OK);
        let result = read_response(response).await;
        assert_eq!(from_utf8(&*result).unwrap(),"Id,Name,Description\n1,Rust,\"Safe, concurrent, performant systems language\"\n");
    }

    #[tokio::test]
    async fn query_arrow_test() -> Result<(), PolarsError> {
        let response = perform_request(
            QueryRequest { query: TEST_QUERY.to_string() },
            QueryResponseFormat::ARROW
        ).await;
        assert_eq!(response.status(), StatusCode::OK);
        let result = read_response(response).await;
        let df = IpcStreamReader::new(Cursor::new(result)).finish()?;
        let id = df.column("Id")?.i32()?.get(0).unwrap();
        let name = df.column("Name")?.utf8()?.get(0).unwrap();
        let description = df.column("Description")?.utf8()?.get(0).unwrap();
        assert_eq!(id, 1);
        assert_eq!(name, "Rust");
        assert_eq!(description, "Safe, concurrent, performant systems language");
        Ok(())


        //assert_eq!(from_utf8(&*result).unwrap(),"{\"Id\":1,\"Name\":\"Rust\",\"Description\":\"Safe, concurrent, performant systems language\"}\n");
    }


    #[tokio::test]
    async fn query_json_gzip_test() {
        let response = perform_request_compress(
            QueryRequest{query: TEST_QUERY.to_string()},
            QueryResponseFormat::JSON,
            true
        ).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers().get(CONTENT_ENCODING).unwrap(),"gzip");
        let result = read_response(response).await;
        assert_eq!(result[0], 0x1fu8);
        assert_eq!(result[1], 0x8bu8);
    }

    async fn perform_request(request:QueryRequest,format:QueryResponseFormat)-> Response{
        perform_request_compress(request,format,false).await
    }
    async fn perform_request_compress(request:QueryRequest,format:QueryResponseFormat,compress:bool) -> Response{
        let json = serde_json::to_string(&request).unwrap();

        let mut builder = Request::builder()
            .method(http::Method::POST)
            .uri("/")
            .header(CONTENT_TYPE,"application/json")
            .header(ACCEPT,format.to_string());
        if compress {
            builder = builder.header(ACCEPT_ENCODING,"gzip");
        }
        app().oneshot(
                builder.body(Body::from(json)).unwrap()
            ).await.unwrap()
    }

    async fn read_response(response:Response) -> Vec<u8>{
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
