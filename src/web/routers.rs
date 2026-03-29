use crate::core::engine::{RecordBatchConsumer, UQueryEngine};
use crate::core::error::UQueryError;
use crate::web::consumers::{ArrowConsumer, WriterConsumer};
use crate::web::request::QueryRequest;
use crate::web::response::QueryResponseFormat;
use crate::web::{
    CONTENT_TYPE_ANY, CONTENT_TYPE_ARROW, CONTENT_TYPE_CSV, CONTENT_TYPE_JSON, CONTENT_TYPE_JSONL,
    CONTENT_TYPE_JSONLINES,
};
use arrow::csv::Writer as CsvWriter;
use arrow::datatypes::SchemaRef;
use arrow::json::{ArrayWriter, LineDelimitedWriter};
use arrow::record_batch::RecordBatch;

use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::header::{ACCEPT, CONTENT_TYPE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::{get, post};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::task::spawn_blocking;
use tokio_util::io::{ReaderStream, SyncIoBridge};
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tracing::error;

/// Wraps a consumer and fires `ready_tx` on the first batch (or on finish for
/// empty results), signaling that DuckDB has produced its first result.
struct FirstBatchNotifier<C: RecordBatchConsumer> {
    inner: C,
    ready_tx: Option<oneshot::Sender<Result<(), String>>>,
}

impl<C: RecordBatchConsumer> RecordBatchConsumer for FirstBatchNotifier<C> {
    fn on_schema(&mut self, schema: SchemaRef) -> Result<(), String> {
        self.inner.on_schema(schema)
    }

    fn on_batch(&mut self, batch: RecordBatch) -> Result<(), String> {
        if let Some(tx) = self.ready_tx.take() {
            let _ = tx.send(Ok(()));
        }
        self.inner.on_batch(batch)
    }

    fn finish(&mut self) -> Result<(), String> {
        // Empty result set: no batches were produced, signal ready here.
        if let Some(tx) = self.ready_tx.take() {
            let _ = tx.send(Ok(()));
        }
        self.inner.finish()
    }
}

pub struct UQueryState {
    pub engine: Arc<dyn UQueryEngine>,
    pub query_timeout: Option<Duration>,
}

pub fn create_router(
    engine: Arc<dyn UQueryEngine>,
    cors_enabled: bool,
    query_timeout: Option<Duration>,
) -> Router {
    let state = Arc::new(UQueryState {
        engine,
        query_timeout,
    });
    let router = Router::new()
        .route("/health", get(|| async { StatusCode::OK }))
        .route("/", post(query))
        .with_state(state)
        .layer(ServiceBuilder::new().layer(CompressionLayer::new()));
    if cors_enabled {
        router.layer(CorsLayer::permissive())
    } else {
        router
    }
}

async fn query(
    State(state): State<Arc<UQueryState>>,
    headers: HeaderMap,
    query_request: QueryRequest,
) -> Result<Response, UQueryError> {
    let format = get_first_compatible_format(&headers).ok_or_else(|| UQueryError {
        status_code: StatusCode::NOT_ACCEPTABLE.as_u16(),
        title: "Unsupported response format".to_string(),
        detail: format!(
            "format [{}] is not supported",
            headers
                .get(ACCEPT)
                .unwrap()
                .to_str()
                .unwrap()
                .to_lowercase()
                .as_str()
        ),
    })?;

    let content_type = format.to_string();
    let sql = query_request.get_sql_query().to_string();
    let (tx, rx) = tokio::io::duplex(1024 * 1024);
    let reader_stream = ReaderStream::new(rx);
    let (ready_tx, ready_rx) = oneshot::channel::<Result<(), String>>();
    let uq_engine = Arc::clone(&state.engine);
    let query_timeout = state.query_timeout;

    spawn_blocking(move || {
        // acquire a connection and defer SQL parsing to execute() — single prepare.
        let mut prepared = uq_engine.prepare(&sql).expect("pool acquire failed");

        // Execute. FirstBatchNotifier fires ready_tx on the first batch (or
        // finish for empty results). If execute() fails before any batch is
        // produced, ready_tx is still Some — we forward the error so the client
        // gets a 400 instead of a dangling request.
        let bridge = SyncIoBridge::new(tx);
        macro_rules! stream_with_notifier {
            ($writer:expr) => {{
                let mut notifier = FirstBatchNotifier {
                    inner: $writer,
                    ready_tx: Some(ready_tx),
                };
                if let Err(e) = prepared.execute(&mut notifier) {
                    if let Some(tx) = notifier.ready_tx.take() {
                        let _ = tx.send(Err(e.clone()));
                    }
                    error!("execution failed: {}", e);
                }
            }};
        }
        match format {
            QueryResponseFormat::Csv => stream_with_notifier!(WriterConsumer::new(CsvWriter::new(bridge))),
            QueryResponseFormat::Json => stream_with_notifier!(WriterConsumer::new(ArrayWriter::new(bridge))),
            QueryResponseFormat::Arrow => stream_with_notifier!(ArrowConsumer::new(bridge)),
            QueryResponseFormat::JsonLINES => {
                stream_with_notifier!(WriterConsumer::new(LineDelimitedWriter::new(bridge)))
            }
        }
    });

    // Timeout covers the time from request start until the first batch is ready.
    // Once streaming begins, results are delivered to completion.
    let ready_result = match query_timeout {
        Some(timeout) => tokio::time::timeout(timeout, ready_rx).await.map_err(|_| {
            UQueryError {
                status_code: StatusCode::REQUEST_TIMEOUT.as_u16(),
                title: "Query Timeout".to_string(),
                detail: format!("no result within {timeout:?}"),
            }
        })?,
        None => ready_rx.await,
    };

    match ready_result {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            return Err(UQueryError {
                status_code: StatusCode::BAD_REQUEST.as_u16(),
                title: "SQL Error".to_string(),
                detail: err,
            });
        }
        Err(_) => {
            return Err(UQueryError {
                status_code: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                title: "Internal Error".to_string(),
                detail: "Query execution task failed unexpectedly".to_string(),
            });
        }
    }

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, content_type)
        .body(Body::from_stream(reader_stream))
        .unwrap())
}

fn get_first_compatible_format(headers: &HeaderMap) -> Option<QueryResponseFormat> {
    let accept_value = headers.get(ACCEPT)?.to_str().unwrap().to_lowercase();
    for format in accept_value.split(",").collect::<Vec<&str>>() {
        match format {
            CONTENT_TYPE_JSON | CONTENT_TYPE_ANY => return Some(QueryResponseFormat::Json),
            CONTENT_TYPE_CSV => return Some(QueryResponseFormat::Csv),
            CONTENT_TYPE_ARROW => return Some(QueryResponseFormat::Arrow),
            CONTENT_TYPE_JSONLINES | CONTENT_TYPE_JSONL => {
                return Some(QueryResponseFormat::JsonLINES);
            }
            _ => {}
        };
    }
    None
}

#[test]
fn content_negotiation_test() {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, "application/json,text/html".parse().unwrap());
    assert!(matches!(
        get_first_compatible_format(&headers),
        Some(QueryResponseFormat::Json)
    ));

    headers.remove(ACCEPT);
    headers.insert(ACCEPT, "application/json".parse().unwrap());
    assert!(matches!(
        get_first_compatible_format(&headers),
        Some(QueryResponseFormat::Json)
    ));

    headers.remove(ACCEPT);
    headers.insert(ACCEPT, "text/csv".parse().unwrap());
    assert!(matches!(
        get_first_compatible_format(&headers),
        Some(QueryResponseFormat::Csv)
    ));

    headers.remove(ACCEPT);
    headers.insert(
        ACCEPT,
        "application/vnd.apache.arrow.stream".parse().unwrap(),
    );
    assert!(matches!(
        get_first_compatible_format(&headers),
        Some(QueryResponseFormat::Arrow)
    ));

    headers.remove(ACCEPT);
    headers.insert(ACCEPT, "application/json,text/csv".parse().unwrap());
    assert!(matches!(
        get_first_compatible_format(&headers),
        Some(QueryResponseFormat::Json)
    ));

    headers.remove(ACCEPT);
    headers.insert(
        ACCEPT,
        "application/xml,application/vnd.apache.arrow.stream"
            .parse()
            .unwrap(),
    );
    assert!(matches!(
        get_first_compatible_format(&headers),
        Some(QueryResponseFormat::Arrow)
    ));

    headers.remove(ACCEPT);
    headers.insert(ACCEPT, "text/html,application/xml".parse().unwrap());
    assert!(matches!(get_first_compatible_format(&headers), None));

    headers.remove(ACCEPT);
    headers.insert(ACCEPT, "application/jsonlines".parse().unwrap());
    assert!(matches!(
        get_first_compatible_format(&headers),
        Some(QueryResponseFormat::JsonLINES)
    ));

    headers.remove(ACCEPT);
    headers.insert(ACCEPT, "application/jsonl".parse().unwrap());
    assert!(matches!(
        get_first_compatible_format(&headers),
        Some(QueryResponseFormat::JsonLINES)
    ));

    headers.remove(ACCEPT);
    headers.insert(ACCEPT, "*/*".parse().unwrap());
    assert!(matches!(
        get_first_compatible_format(&headers),
        Some(QueryResponseFormat::Json)
    ));

    headers.remove(ACCEPT);
    assert!(matches!(get_first_compatible_format(&headers), None));
}
