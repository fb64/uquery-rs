use crate::core::engine::{RecordBatchConsumer, UQueryEngine};
use crate::core::error::UQueryError;
use crate::web::consumers::{ArrowConsumer, WriterConsumer};
use crate::web::request::QueryRequest;
use crate::web::response::{ArrowCodec, QueryResponseFormat};
use crate::web::{
    CONTENT_TYPE_ANY, CONTENT_TYPE_ARROW, CONTENT_TYPE_ARROW_WITH_CODECS, CONTENT_TYPE_CSV,
    CONTENT_TYPE_JSON, CONTENT_TYPE_JSONL, CONTENT_TYPE_JSONLINES,
};
use arrow::csv::Writer as CsvWriter;
use arrow::datatypes::SchemaRef;
use arrow::ipc::CompressionType;
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
use tower_http::compression::predicate::{DefaultPredicate, NotForContentType, Predicate};
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
    // Arrow responses with negotiated IPC buffer compression are already compressed,
    // so the outer gzip layer skips them.
    let router = Router::new()
        .route("/health", get(|| async { StatusCode::OK }))
        .route("/", post(query))
        .with_state(state)
        .layer(
            ServiceBuilder::new().layer(
                CompressionLayer::new().compress_when(
                    DefaultPredicate::new()
                        .and(NotForContentType::const_new(CONTENT_TYPE_ARROW_WITH_CODECS)),
                ),
            ),
        );
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
                .map(|accept| String::from_utf8_lossy(accept.as_bytes()).to_lowercase())
                .unwrap_or_default()
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
            QueryResponseFormat::Csv => {
                stream_with_notifier!(WriterConsumer::new(CsvWriter::new(bridge)))
            }
            QueryResponseFormat::Json => {
                stream_with_notifier!(WriterConsumer::new(ArrayWriter::new(bridge)))
            }
            QueryResponseFormat::Arrow(codec) => {
                stream_with_notifier!(ArrowConsumer::new(bridge, codec.map(CompressionType::from)))
            }
            QueryResponseFormat::JsonLINES => {
                stream_with_notifier!(WriterConsumer::new(LineDelimitedWriter::new(bridge)))
            }
        }
    });

    // Timeout covers the time from request start until the first batch is ready.
    // Once streaming begins, results are delivered to completion.
    let ready_result = match query_timeout {
        Some(timeout) => {
            tokio::time::timeout(timeout, ready_rx)
                .await
                .map_err(|_| UQueryError {
                    status_code: StatusCode::REQUEST_TIMEOUT.as_u16(),
                    title: "Query Timeout".to_string(),
                    detail: format!("no result within {timeout:?}"),
                })?
        }
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
    // No Accept header means any media type is acceptable (RFC 9110), so default to JSON.
    let Some(accept) = headers.get(ACCEPT) else {
        return Some(QueryResponseFormat::Json);
    };
    let accept_value = accept.to_str().ok()?.to_lowercase();
    split_unquoted(&accept_value, ',').find_map(|media_range| {
        let mut parts = split_unquoted(media_range, ';');
        match parts.next()?.trim() {
            CONTENT_TYPE_JSON | CONTENT_TYPE_ANY => Some(QueryResponseFormat::Json),
            CONTENT_TYPE_CSV => Some(QueryResponseFormat::Csv),
            CONTENT_TYPE_ARROW => Some(QueryResponseFormat::Arrow(negotiate_arrow_codec(parts))),
            CONTENT_TYPE_JSONLINES | CONTENT_TYPE_JSONL => Some(QueryResponseFormat::JsonLINES),
            _ => None,
        }
    })
}

/// Picks the first supported codec from the `codecs` media type parameter, in client order.
/// Unknown codecs are skipped: uncompressed IPC is always readable by Arrow clients.
fn negotiate_arrow_codec<'a>(mut params: impl Iterator<Item = &'a str>) -> Option<ArrowCodec> {
    let codecs = params.find_map(|param| {
        let (name, value) = param.split_once('=')?;
        (name.trim() == "codecs").then_some(value)
    })?;
    codecs
        .trim()
        .trim_matches('"')
        .split(',')
        .find_map(|codec| ArrowCodec::from_name(codec.trim()))
}

/// Splits on `delimiter` except inside double quotes, so `codecs="zstd, lz4"` stays whole.
fn split_unquoted(value: &str, delimiter: char) -> impl Iterator<Item = &str> {
    let mut in_quotes = false;
    value.split(move |c: char| {
        if c == '"' {
            in_quotes = !in_quotes;
        }
        c == delimiter && !in_quotes
    })
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
        Some(QueryResponseFormat::Arrow(None))
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
        Some(QueryResponseFormat::Arrow(None))
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
    assert!(matches!(
        get_first_compatible_format(&headers),
        Some(QueryResponseFormat::Json)
    ));
}

#[test]
fn arrow_codec_negotiation_test() {
    let cases: [(&str, Option<ArrowCodec>); 9] = [
        ("application/vnd.apache.arrow.stream", None),
        (
            r#"application/vnd.apache.arrow.stream; codecs="zstd, lz4""#,
            Some(ArrowCodec::Zstd),
        ),
        (
            r#"application/vnd.apache.arrow.stream; codecs="lz4, zstd""#,
            Some(ArrowCodec::Lz4),
        ),
        (
            "application/vnd.apache.arrow.stream; codecs=lz4",
            Some(ArrowCodec::Lz4),
        ),
        (
            r#"application/vnd.apache.arrow.stream; codecs="brotli, zstd""#,
            Some(ArrowCodec::Zstd),
        ),
        (
            r#"application/vnd.apache.arrow.stream; codecs="brotli""#,
            None,
        ),
        (
            r#"Application/Vnd.Apache.Arrow.Stream; Codecs="ZSTD""#,
            Some(ArrowCodec::Zstd),
        ),
        (
            "application/vnd.apache.arrow.stream; q=0.9; codecs=lz4",
            Some(ArrowCodec::Lz4),
        ),
        (
            r#"text/html, application/vnd.apache.arrow.stream; codecs="zstd, lz4", application/json"#,
            Some(ArrowCodec::Zstd),
        ),
    ];
    for (accept, codec) in cases {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, accept.parse().unwrap());
        assert_eq!(
            get_first_compatible_format(&headers),
            Some(QueryResponseFormat::Arrow(codec)),
            "Accept: {accept}"
        );
    }
}

#[test]
fn media_type_parameters_are_ignored_test() {
    let mut headers = HeaderMap::new();
    headers.insert(
        ACCEPT,
        "text/html, application/json; q=0.9".parse().unwrap(),
    );
    assert_eq!(
        get_first_compatible_format(&headers),
        Some(QueryResponseFormat::Json)
    );
}
