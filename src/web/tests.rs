use crate::core::duckdb::{DuckDbEngine, UQ_ATTACHED_DB_NAME};
use crate::core::engine::{ExecutableQuery, QueryCanceller, RecordBatchConsumer, UQueryEngine};
use crate::core::error::EngineError;
use crate::web::request::QueryRequest;
use crate::web::response::QueryResponseFormat;
use crate::web::routers::create_router;
use arrow::ipc::{CompressionType, root_as_message};
use axum::body::Body;
use axum::http;
use axum::http::header::{
    ACCEPT, ACCEPT_ENCODING, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN,
    CONTENT_ENCODING, CONTENT_TYPE, ORIGIN,
};
use axum::http::{HeaderValue, Request, StatusCode};
use axum::response::Response;
use duckdb::Connection;
use futures_util::TryStreamExt;
use polars::error::PolarsError;
use polars_io::SerReader;
use polars_io::ipc::IpcStreamReader;
use serde_json::Value;
use std::io::Cursor;
use std::str::from_utf8;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

struct SlowEngine(Duration);

impl UQueryEngine for SlowEngine {
    fn prepare(&self, _sql: &str) -> Result<Box<dyn ExecutableQuery>, EngineError> {
        std::thread::sleep(self.0);
        Ok(Box::new(SlowQuery))
    }
}

struct SlowQuery;

impl ExecutableQuery for SlowQuery {
    fn execute(&mut self, consumer: &mut dyn RecordBatchConsumer) -> Result<(), EngineError> {
        consumer.finish()
    }

    fn canceller(&self) -> Arc<dyn QueryCanceller> {
        Arc::new(NoopCanceller)
    }
}

struct NoopCanceller;

impl QueryCanceller for NoopCanceller {
    fn cancel(&self) {}
}

const TEST_QUERY: &str = "SELECT * FROM (VALUES (1,'Rust','Safe, concurrent, performant systems language')) Language(Id,Name,Description)";

#[tokio::test]
async fn query_json_test() {
    let response = perform_json_request(
        QueryRequest::new(TEST_QUERY.to_string()),
        QueryResponseFormat::Json,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let result = read_response(response).await;
    assert_eq!(
        from_utf8(&result).unwrap(),
        "[{\"Id\":1,\"Name\":\"Rust\",\"Description\":\"Safe, concurrent, performant systems language\"}]"
    );
}

#[tokio::test]
async fn query_text_plain_json_test() {
    let response =
        perform_plain_text_request(TEST_QUERY.to_string(), QueryResponseFormat::Json).await;
    assert_eq!(response.status(), StatusCode::OK);
    let result = read_response(response).await;
    assert_eq!(
        from_utf8(&result).unwrap(),
        "[{\"Id\":1,\"Name\":\"Rust\",\"Description\":\"Safe, concurrent, performant systems language\"}]"
    );
}

#[tokio::test]
async fn query_without_accept_defaults_to_json_test() {
    let request = Request::builder()
        .method(http::Method::POST)
        .uri("/")
        .header(CONTENT_TYPE, "text/plain")
        .body(Body::from(TEST_QUERY))
        .unwrap();
    let response = create_router(make_engine(false), false, None, None)
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let result = read_response(response).await;
    assert_eq!(
        from_utf8(&result).unwrap(),
        "[{\"Id\":1,\"Name\":\"Rust\",\"Description\":\"Safe, concurrent, performant systems language\"}]"
    );
}

#[tokio::test]
async fn query_with_non_ascii_accept_is_not_acceptable_test() {
    let request = Request::builder()
        .method(http::Method::POST)
        .uri("/")
        .header(CONTENT_TYPE, "text/plain")
        .header(ACCEPT, HeaderValue::from_bytes(b"text/caf\xe9").unwrap())
        .body(Body::from(TEST_QUERY))
        .unwrap();
    let response = create_router(make_engine(false), false, None, None)
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);
}

#[tokio::test]
async fn query_csv_test() {
    let response = perform_json_request(
        QueryRequest::new(TEST_QUERY.to_string()),
        QueryResponseFormat::Csv,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let result = read_response(response).await;
    assert_eq!(
        from_utf8(&result).unwrap(),
        "Id,Name,Description\n1,Rust,\"Safe, concurrent, performant systems language\"\n"
    );
}

#[tokio::test]
async fn query_arrow_test() -> Result<(), PolarsError> {
    let response = perform_json_request(
        QueryRequest::new(TEST_QUERY.to_string()),
        QueryResponseFormat::Arrow(None),
    )
    .await;
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
}

#[tokio::test]
async fn query_arrow_zstd_negotiated_test() -> Result<(), PolarsError> {
    let response = perform_arrow_request(
        r#"application/vnd.apache.arrow.stream; codecs="zstd, lz4""#,
        Some("gzip"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(CONTENT_TYPE).unwrap(),
        "application/vnd.apache.arrow.stream; codecs=zstd"
    );
    // IPC buffers are already compressed, so the outer gzip layer must skip them.
    assert!(response.headers().get(CONTENT_ENCODING).is_none());
    let result = read_response(response).await;
    assert_eq!(first_batch_codec(&result), Some(CompressionType::ZSTD));
    let df = IpcStreamReader::new(Cursor::new(result)).finish()?;
    assert_eq!(df.column("Id")?.i32()?.get(0).unwrap(), 1);
    Ok(())
}

#[tokio::test]
async fn query_arrow_lz4_negotiated_test() -> Result<(), PolarsError> {
    let response =
        perform_arrow_request("application/vnd.apache.arrow.stream; codecs=lz4", None).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(CONTENT_TYPE).unwrap(),
        "application/vnd.apache.arrow.stream; codecs=lz4"
    );
    let result = read_response(response).await;
    assert_eq!(first_batch_codec(&result), Some(CompressionType::LZ4_FRAME));
    let df = IpcStreamReader::new(Cursor::new(result)).finish()?;
    assert_eq!(df.column("Id")?.i32()?.get(0).unwrap(), 1);
    Ok(())
}

#[tokio::test]
async fn query_arrow_without_codecs_is_uncompressed_test() {
    let response = perform_arrow_request("application/vnd.apache.arrow.stream", None).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(CONTENT_TYPE).unwrap(),
        "application/vnd.apache.arrow.stream"
    );
    let result = read_response(response).await;
    assert_eq!(first_batch_codec(&result), None);
}

#[tokio::test]
async fn query_arrow_without_codecs_uses_http_gzip_test() {
    let response = perform_arrow_request("application/vnd.apache.arrow.stream", Some("gzip")).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get(CONTENT_ENCODING).unwrap(), "gzip");
    let result = read_response(response).await;
    assert_eq!(result[0], 0x1fu8);
    assert_eq!(result[1], 0x8bu8);
}

#[tokio::test]
async fn query_json_gzip_test() {
    let response = perform_json_request_compress(
        QueryRequest::new(TEST_QUERY.to_string()),
        QueryResponseFormat::Json,
        true,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get(CONTENT_ENCODING).unwrap(), "gzip");
    let result = read_response(response).await;
    assert_eq!(result[0], 0x1fu8);
    assert_eq!(result[1], 0x8bu8);
}

#[tokio::test]
async fn query_attached_db_test() {
    let request = QueryRequest::new("SELECT * from language order by id".to_string());
    let json = serde_json::to_string(&request).unwrap();

    let builder = Request::builder()
        .method(http::Method::POST)
        .uri("/")
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, QueryResponseFormat::Json.to_string());

    let conn = Connection::open_in_memory().unwrap();
    conn.execute(
        format!("ATTACH 'tests/test.db' as {UQ_ATTACHED_DB_NAME};").as_str(),
        [],
    )
    .unwrap();
    let engine: Arc<dyn UQueryEngine> = Arc::new(DuckDbEngine::new(conn, true, 2).unwrap());
    let response = create_router(engine, false, None, None)
        .oneshot(builder.body(Body::from(json)).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let result = read_response(response).await;
    let response_string = from_utf8(&result).unwrap();
    let json_array: Vec<Value> = serde_json::from_str(response_string).unwrap();
    assert_eq!(json_array.len(), 10);
    assert_eq!(json_array[0].get("id").unwrap().as_i64().unwrap(), 1);
    assert_eq!(json_array[0].get("name").unwrap().as_str().unwrap(), "Rust");
}

#[tokio::test]
async fn cors_enabled_test() {
    let builder = Request::builder()
        .method(http::Method::OPTIONS)
        .uri("/")
        .header(ACCESS_CONTROL_ALLOW_METHODS, "POST")
        .header(ORIGIN, "https://origin.com");

    let conn = Connection::open_in_memory().unwrap();
    let engine: Arc<dyn UQueryEngine> = Arc::new(DuckDbEngine::new(conn, false, 2).unwrap());
    let response = create_router(engine, true, None, None)
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(),
        "*"
    );
    assert_eq!(
        response
            .headers()
            .get(ACCESS_CONTROL_ALLOW_METHODS)
            .unwrap(),
        "*"
    );
}

#[tokio::test]
async fn query_sql_error_test() {
    let response = perform_json_request(
        QueryRequest::new("bad command".to_string()),
        QueryResponseFormat::Json,
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let result = read_response(response).await;
    let error: Value = serde_json::from_str(from_utf8(&result).unwrap()).unwrap();
    assert_eq!(error["status"].as_u64().unwrap(), 400);
    assert_eq!(error["title"], "SQL Error");
    assert!(!error["detail"].to_string().is_empty());
}

#[tokio::test]
async fn read_csv_test() {
    let response = perform_json_request(
        QueryRequest::new("select * from read_csv('tests/test.csv')".to_string()),
        QueryResponseFormat::Json,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let result = read_response(response).await;
    let response_string = from_utf8(&result).unwrap();
    let json_array: Vec<Value> = serde_json::from_str(response_string).unwrap();
    assert_eq!(json_array.len(), 2);
    assert_eq!(json_array[0].get("f_str").unwrap().as_str().unwrap(), "abc");
    assert_eq!(json_array[0].get("f_int").unwrap().as_i64().unwrap(), 123);
    assert_eq!(
        json_array[0].get("f_float").unwrap().as_f64().unwrap(),
        4.56
    );
}

#[tokio::test]
async fn read_parquet_test() {
    let response = perform_json_request(
        QueryRequest::new("select * from 'tests/test.zstd.parquet'".to_string()),
        QueryResponseFormat::Json,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);

    let result = read_response(response).await;
    let response_string = from_utf8(&result).unwrap();
    let json_array: Vec<Value> = serde_json::from_str(response_string).unwrap();

    assert_eq!(json_array.len(), 2);
    assert_eq!(json_array[0].get("f_str").unwrap().as_str().unwrap(), "abc");
    assert_eq!(json_array[0].get("f_int").unwrap().as_i64().unwrap(), 123);
    assert_eq!(
        json_array[0].get("f_float").unwrap().as_f64().unwrap(),
        4.56
    );
}

#[tokio::test]
async fn read_json_test() {
    let response = perform_json_request(
        QueryRequest::new("select * from 'tests/test.jsonl'".to_string()),
        QueryResponseFormat::Json,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);

    let result = read_response(response).await;
    let response_string = from_utf8(&result).unwrap();
    let json_array: Vec<Value> = serde_json::from_str(response_string).unwrap();

    assert_eq!(json_array.len(), 2);
    assert_eq!(json_array[0].get("f_str").unwrap().as_str().unwrap(), "abc");
    assert_eq!(json_array[0].get("f_int").unwrap().as_i64().unwrap(), 123);
    assert_eq!(
        json_array[0].get("f_float").unwrap().as_f64().unwrap(),
        4.56
    );
}

#[tokio::test]
async fn text_plain_request_test() {
    let response = perform_plain_text_request(
        "select * from 'tests/test.jsonl'".to_string(),
        QueryResponseFormat::Json,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);

    let result = read_response(response).await;
    let response_string = from_utf8(&result).unwrap();
    let json_array: Vec<Value> = serde_json::from_str(response_string).unwrap();

    assert_eq!(json_array.len(), 2);
    assert_eq!(json_array[0].get("f_str").unwrap().as_str().unwrap(), "abc");
    assert_eq!(json_array[0].get("f_int").unwrap().as_i64().unwrap(), 123);
    assert_eq!(
        json_array[0].get("f_float").unwrap().as_f64().unwrap(),
        4.56
    );
}

#[tokio::test]
async fn read_jsonlines_test() {
    let response = perform_json_request(
        QueryRequest::new("select * from 'tests/test.jsonl'".to_string()),
        QueryResponseFormat::JsonLINES,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);

    let result = read_response(response).await;
    let response_string = from_utf8(&result).unwrap().lines().collect::<Vec<&str>>();
    let json_first: Value = serde_json::from_str(response_string.first().unwrap()).unwrap();

    assert_eq!(response_string.len(), 2);
    assert_eq!(json_first.get("f_str").unwrap().as_str().unwrap(), "abc");
    assert_eq!(json_first.get("f_int").unwrap().as_i64().unwrap(), 123);
    assert_eq!(json_first.get("f_float").unwrap().as_f64().unwrap(), 4.56);
}

#[tokio::test]
/*
   The following macro table has been created in the tests/test.db DuckDB database file
   create macro table test() as select * from 'tests/test.zstd.parquet'
*/
async fn query_attached_macro_table_test() {
    let request = QueryRequest::new("SELECT * from test()".to_string());
    let json = serde_json::to_string(&request).unwrap();

    let builder = Request::builder()
        .method(http::Method::POST)
        .uri("/")
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, QueryResponseFormat::Json.to_string());

    let conn = Connection::open_in_memory().unwrap();
    conn.execute(
        format!("ATTACH 'tests/test.db' as {UQ_ATTACHED_DB_NAME};").as_str(),
        [],
    )
    .unwrap();
    let engine: Arc<dyn UQueryEngine> = Arc::new(DuckDbEngine::new(conn, true, 2).unwrap());
    let response = create_router(engine, false, None, None)
        .oneshot(builder.body(Body::from(json)).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let result = read_response(response).await;
    let response_string = from_utf8(&result).unwrap();
    let json_array: Vec<Value> = serde_json::from_str(response_string).unwrap();
    assert_eq!(json_array.len(), 2);
    assert_eq!(json_array[0].get("f_str").unwrap().as_str().unwrap(), "abc");
    assert_eq!(json_array[0].get("f_int").unwrap().as_i64().unwrap(), 123);
    assert_eq!(
        json_array[0].get("f_float").unwrap().as_f64().unwrap(),
        4.56
    );
}

#[tokio::test]
async fn query_timeout_test() {
    let engine: Arc<dyn UQueryEngine> = Arc::new(SlowEngine(Duration::from_millis(500)));
    let request = Request::builder()
        .method(http::Method::POST)
        .uri("/")
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, QueryResponseFormat::Json.to_string())
        .body(Body::from(
            serde_json::to_string(&QueryRequest::new("SELECT 1".to_string())).unwrap(),
        ))
        .unwrap();
    let response = create_router(engine, false, Some(Duration::from_millis(50)), None)
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::REQUEST_TIMEOUT);
}

#[tokio::test]
async fn query_timeout_cancels_query_test() {
    // A single pooled connection: the next query only runs if the timed-out
    // one was cancelled and released its connection.
    let engine: Arc<dyn UQueryEngine> =
        Arc::new(DuckDbEngine::new(Connection::open_in_memory().unwrap(), false, 1).unwrap());
    let router = create_router(engine, false, Some(Duration::from_millis(500)), None);
    let request = |sql: &str| {
        Request::builder()
            .method(http::Method::POST)
            .uri("/")
            .header(CONTENT_TYPE, "text/plain")
            .body(Body::from(sql.to_string()))
            .unwrap()
    };

    let response = router
        .clone()
        .oneshot(request("SELECT count(*) FROM range(1000000000000)"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::REQUEST_TIMEOUT);

    let response = router
        .oneshot(request("SELECT 42 AS answer"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let result = read_response(response).await;
    assert_eq!(from_utf8(&result).unwrap(), "[{\"answer\":42}]");
}

#[tokio::test]
async fn query_api_key_test() {
    let router = create_router(make_engine(false), false, None, Some("secret".to_string()));
    let request = |header: Option<(&str, &str)>| {
        let mut builder = Request::builder()
            .method(http::Method::POST)
            .uri("/")
            .header(CONTENT_TYPE, "text/plain");
        if let Some((name, value)) = header {
            builder = builder.header(name, value);
        }
        builder.body(Body::from(TEST_QUERY)).unwrap()
    };

    for header in [
        None,
        Some(("authorization", "Bearer wrong")),
        Some(("x-api-key", "wrong")),
        Some(("authorization", "Basic secret")),
    ] {
        let response = router.clone().oneshot(request(header)).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{header:?}");
        assert_eq!(
            response.headers().get("www-authenticate").unwrap(),
            "Bearer"
        );
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "application/problem+json"
        );
    }

    for header in [("authorization", "Bearer secret"), ("x-api-key", "secret")] {
        let response = router.clone().oneshot(request(Some(header))).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{header:?}");
    }

    let health = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let response = router.oneshot(health).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

fn make_engine(attached: bool) -> Arc<dyn UQueryEngine> {
    Arc::new(DuckDbEngine::new(Connection::open_in_memory().unwrap(), attached, 2).unwrap())
}

async fn perform_json_request(request: QueryRequest, format: QueryResponseFormat) -> Response {
    perform_json_request_compress(request, format, false).await
}

async fn perform_json_request_compress(
    request: QueryRequest,
    format: QueryResponseFormat,
    compress: bool,
) -> Response {
    let json = serde_json::to_string(&request).unwrap();

    let mut builder = Request::builder()
        .method(http::Method::POST)
        .uri("/")
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, format.to_string());
    if compress {
        builder = builder.header(ACCEPT_ENCODING, "gzip");
    }
    create_router(make_engine(false), false, None, None)
        .oneshot(builder.body(Body::from(json)).unwrap())
        .await
        .unwrap()
}

async fn perform_arrow_request(accept: &str, accept_encoding: Option<&str>) -> Response {
    let mut builder = Request::builder()
        .method(http::Method::POST)
        .uri("/")
        .header(CONTENT_TYPE, "text/plain")
        .header(ACCEPT, accept);
    if let Some(encoding) = accept_encoding {
        builder = builder.header(ACCEPT_ENCODING, encoding);
    }
    create_router(make_engine(false), false, None, None)
        .oneshot(builder.body(Body::from(TEST_QUERY)).unwrap())
        .await
        .unwrap()
}

/// Codec of the first record batch of an Arrow IPC stream, read from its message header.
/// Each IPC message is: 0xFFFFFFFF continuation, i32 metadata length, metadata, body.
fn first_batch_codec(ipc: &[u8]) -> Option<CompressionType> {
    let mut offset = 0;
    loop {
        let metadata_len =
            i32::from_le_bytes(ipc[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let metadata = &ipc[offset + 8..offset + 8 + metadata_len];
        let message = root_as_message(metadata).unwrap();
        if let Some(batch) = message.header_as_record_batch() {
            return batch.compression().map(|compression| compression.codec());
        }
        offset += 8 + metadata_len + message.bodyLength() as usize;
    }
}

async fn perform_plain_text_request(sql: String, format: QueryResponseFormat) -> Response {
    let builder = Request::builder()
        .method(http::Method::POST)
        .uri("/")
        .header(CONTENT_TYPE, "text/plain")
        .header(ACCEPT, format.to_string());
    create_router(make_engine(false), false, None, None)
        .oneshot(builder.body(Body::from(sql)).unwrap())
        .await
        .unwrap()
}

async fn read_response(response: Response) -> Vec<u8> {
    response
        .into_body()
        .into_data_stream()
        .map_ok(|bytes| bytes.to_vec())
        .try_fold(Vec::new(), |mut acc, item| {
            acc.extend_from_slice(&item);
            async move { Ok(acc) }
        })
        .await
        .unwrap_or_else(|e| {
            eprintln!("Error: {}", e);
            Vec::new()
        })
}
