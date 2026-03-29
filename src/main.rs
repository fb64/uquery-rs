use crate::core::duckdb::DuckDbEngine;
use crate::core::engine::UQueryEngine;
use duckdb::Connection;
use pingora::prelude::{Server, http_proxy_service};

use crate::web::proxy::UIProxyService;
use std::sync::Arc;
use tokio::signal;
use tokio::time::Instant;
use tracing::{debug, info};

mod cli;
pub mod core;
mod web;

fn main() {
    let cli_options = cli::options::parse();
    let start = Instant::now();
    let addr = format!("{}:{}", cli_options.addr, cli_options.port);
    let conn = Connection::open_in_memory().unwrap();
    for init_query in cli_options.init_script() {
        conn.execute(init_query.as_str(), []).unwrap();
    }
    let engine: Arc<dyn UQueryEngine> = Arc::new(
        DuckDbEngine::new(conn, cli_options.db_file.is_some(), cli_options.pool_size).unwrap(),
    );

    let tk_runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    if cli_options.duckdb_ui {
        tk_runtime.spawn_blocking(move || {
            start_duckdb_ui_proxy(cli_options.duckdb_ui_port);
        });
    }

    tk_runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
        info!("uQuery server started in {:?}", start.elapsed());
        debug!("listening on {}", addr);
        let query_timeout = match cli_options.query_timeout_secs {
            0 => None,
            secs => Some(std::time::Duration::from_secs(secs)),
        };
        let router = web::routers::create_router(engine, cli_options.cors_enabled, query_timeout);
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown_signal())
            .await
            .unwrap();
    });
}

fn start_duckdb_ui_proxy(ui_port: u16) {
    let mut server = Server::new(None).unwrap();
    server.bootstrap();
    let service: UIProxyService = UIProxyService;
    let mut app = http_proxy_service(&server.configuration, service);
    app.add_tcp(format!("0.0.0.0:{ui_port}").as_str());
    server.add_service(app);
    info!("DuckDB UI Proxy server started on port: {ui_port}");
    server.run_forever()
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    debug!("Shutting down uQuery server");
}

#[cfg(test)]
mod tests {
    use crate::cli::options::UQ_ATTACHED_DB_NAME;
    use crate::core::duckdb::DuckDbEngine;
    use crate::core::engine::{ExecutableQuery, RecordBatchConsumer, UQueryEngine};
    use crate::web::request::QueryRequest;
    use crate::web::response::QueryResponseFormat;
    use crate::web::routers::create_router;
    use axum::body::Body;
    use axum::http;
    use axum::http::header::{
        ACCEPT, ACCEPT_ENCODING, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN,
        CONTENT_ENCODING, CONTENT_TYPE, ORIGIN,
    };
    use axum::http::{Request, StatusCode};
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
        fn prepare(&self, _sql: &str) -> Result<Box<dyn ExecutableQuery>, String> {
            std::thread::sleep(self.0);
            Ok(Box::new(SlowQuery))
        }
    }

    struct SlowQuery;

    impl ExecutableQuery for SlowQuery {
        fn execute(&mut self, consumer: &mut dyn RecordBatchConsumer) -> Result<(), String> {
            consumer.finish()
        }
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
            from_utf8(&*result).unwrap(),
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
            from_utf8(&*result).unwrap(),
            "[{\"Id\":1,\"Name\":\"Rust\",\"Description\":\"Safe, concurrent, performant systems language\"}]"
        );
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
            from_utf8(&*result).unwrap(),
            "Id,Name,Description\n1,Rust,\"Safe, concurrent, performant systems language\"\n"
        );
    }

    #[tokio::test]
    async fn query_arrow_test() -> Result<(), PolarsError> {
        let response = perform_json_request(
            QueryRequest::new(TEST_QUERY.to_string()),
            QueryResponseFormat::Arrow,
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
        let engine: Arc<dyn UQueryEngine> =
            Arc::new(DuckDbEngine::new(conn, true, 2).unwrap());
        let response = create_router(engine, false, None)
            .oneshot(builder.body(Body::from(json)).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let result = read_response(response).await;
        let response_string = from_utf8(&*result).unwrap();
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
        let engine: Arc<dyn UQueryEngine> =
            Arc::new(DuckDbEngine::new(conn, false, 2).unwrap());
        let response = create_router(engine, true, None)
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
        let error: Value = serde_json::from_str(from_utf8(&*result).unwrap()).unwrap();
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
        let response_string = from_utf8(&*result).unwrap();
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
        let response_string = from_utf8(&*result).unwrap();
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
        let response_string = from_utf8(&*result).unwrap();
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
        let response_string = from_utf8(&*result).unwrap();
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
        let response_string = from_utf8(&*result).unwrap().lines().collect::<Vec<&str>>();
        let json_first: Value = serde_json::from_str(response_string.get(0).unwrap()).unwrap();

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
        let engine: Arc<dyn UQueryEngine> =
            Arc::new(DuckDbEngine::new(conn, true, 2).unwrap());
        let response = create_router(engine, false, None)
            .oneshot(builder.body(Body::from(json)).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let result = read_response(response).await;
        let response_string = from_utf8(&*result).unwrap();
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
        let response = create_router(engine, false, Some(Duration::from_millis(50)))
            .oneshot(request)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::REQUEST_TIMEOUT);
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
        create_router(make_engine(false), false, None)
            .oneshot(builder.body(Body::from(json)).unwrap())
            .await
            .unwrap()
    }

    async fn perform_plain_text_request(sql: String, format: QueryResponseFormat) -> Response {
        let builder = Request::builder()
            .method(http::Method::POST)
            .uri("/")
            .header(CONTENT_TYPE, "text/plain")
            .header(ACCEPT, format.to_string());
        create_router(make_engine(false), false, None)
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
}
