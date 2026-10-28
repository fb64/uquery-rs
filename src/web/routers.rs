use crate::core::engine::{QueryCanceller, RecordBatchConsumer, UQueryEngine};
use crate::core::error::{EngineError, UQueryError};
use crate::web::auth::{reject_browser_requests, require_api_key};
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

use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::header::{ACCEPT, CONTENT_TYPE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Router, middleware};
use futures_util::{Stream, StreamExt};
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::DuplexStream;
use tokio::sync::oneshot;
use tokio::task::spawn_blocking;
use tokio::time::{Instant, Sleep};
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
    ready_tx: Option<oneshot::Sender<Result<(), EngineError>>>,
}

impl<C: RecordBatchConsumer> RecordBatchConsumer for FirstBatchNotifier<C> {
    fn on_schema(&mut self, schema: SchemaRef) -> Result<(), EngineError> {
        self.inner.on_schema(schema)
    }

    fn on_batch(&mut self, batch: RecordBatch) -> Result<(), EngineError> {
        if let Some(tx) = self.ready_tx.take() {
            let _ = tx.send(Ok(()));
        }
        self.inner.on_batch(batch)
    }

    fn finish(&mut self) -> Result<(), EngineError> {
        // Empty result set: no batches were produced, signal ready here.
        if let Some(tx) = self.ready_tx.take() {
            let _ = tx.send(Ok(()));
        }
        self.inner.finish()
    }
}

/// Cancels the query when the handler ends before the first batch: on timeout,
/// or when the client disconnects and axum drops the handler future.
struct CancelOnDrop(Option<oneshot::Receiver<Arc<dyn QueryCanceller>>>);

impl CancelOnDrop {
    /// Stop cancelling on drop and return the canceller, if the query got one.
    fn disarm(mut self) -> Option<Arc<dyn QueryCanceller>> {
        self.0
            .take()
            .and_then(|mut canceller_rx| canceller_rx.try_recv().ok())
    }
}

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some(mut canceller_rx) = self.0.take()
            && let Ok(canceller) = canceller_rx.try_recv()
        {
            canceller.cancel();
        }
    }
}

/// Ends a response body with an error once `deadline` passes, and cancels the query.
///
/// Erroring (instead of ending the body) makes the server abort the response, so the
/// client cannot mistake a truncated result for a complete one. Dropping the body also
/// unblocks a query thread waiting on a client that stopped reading.
struct DeadlineStream<S> {
    inner: Pin<Box<S>>,
    sleep: Pin<Box<Sleep>>,
    canceller: Option<Arc<dyn QueryCanceller>>,
    expired: bool,
}

impl<S: Stream<Item = io::Result<Bytes>>> Stream for DeadlineStream<S> {
    type Item = io::Result<Bytes>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.expired {
            return Poll::Ready(None);
        }
        // Checked first: a fast stream is always ready and would never expire.
        if this.sleep.as_mut().poll(cx).is_ready() {
            this.expired = true;
            if let Some(canceller) = this.canceller.take() {
                canceller.cancel();
            }
            return Poll::Ready(Some(Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "query exceeded the maximum duration",
            ))));
        }
        this.inner.as_mut().poll_next(cx)
    }
}

/// Builds the response body from the query output. The body fails if the query
/// fails after streaming started or exceeds `deadline`.
fn response_body(
    reader: ReaderStream<DuplexStream>,
    done_rx: oneshot::Receiver<Result<(), EngineError>>,
    deadline: Option<(Instant, Option<Arc<dyn QueryCanceller>>)>,
) -> Body {
    let completion = futures_util::stream::once(async move {
        match done_rx.await {
            Ok(Ok(())) => None,
            Ok(Err(e)) => Some(Err(io::Error::other(e.to_string()))),
            Err(_) => Some(Err(io::Error::other("query task failed"))),
        }
    })
    .filter_map(std::future::ready);
    let stream = reader.chain(completion);
    match deadline {
        None => Body::from_stream(stream),
        Some((deadline, canceller)) => Body::from_stream(DeadlineStream {
            inner: Box::pin(stream),
            sleep: Box::pin(tokio::time::sleep_until(deadline)),
            canceller,
            expired: false,
        }),
    }
}

/// Options of the µQuery HTTP API.
#[derive(Clone, Debug, Default)]
pub struct RouterOptions {
    /// Enable permissive CORS. When disabled, requests from web browsers are rejected.
    pub cors_enabled: bool,
    /// Maximum time until the first result batch; the query is cancelled after it.
    pub query_timeout: Option<Duration>,
    /// Maximum total time of a query, including streaming its result. A response
    /// still streaming at that point is aborted.
    pub max_query_duration: Option<Duration>,
    /// Require this API key on queries.
    pub api_key: Option<String>,
}

pub struct UQueryState {
    pub engine: Arc<dyn UQueryEngine>,
    pub query_timeout: Option<Duration>,
    pub max_query_duration: Option<Duration>,
}

pub fn create_router(engine: Arc<dyn UQueryEngine>, options: RouterOptions) -> Router {
    let RouterOptions {
        cors_enabled,
        query_timeout,
        max_query_duration,
        api_key,
    } = options;
    let state = Arc::new(UQueryState {
        engine,
        query_timeout,
        max_query_duration,
    });
    // Arrow responses with negotiated IPC buffer compression are already compressed,
    // so the outer gzip layer skips them.
    let mut query_route = post(query);
    if !cors_enabled {
        query_route = query_route.route_layer(middleware::from_fn(reject_browser_requests));
    }
    if let Some(api_key) = api_key {
        query_route = query_route.route_layer(middleware::from_fn_with_state(
            Arc::<str>::from(api_key),
            require_api_key,
        ));
    }
    let router = Router::new()
        .route("/health", get(|| async { StatusCode::OK }))
        .route("/", query_route)
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

    let start = Instant::now();
    let content_type = format.to_string();
    let sql = query_request.get_sql_query().to_string();
    let (tx, rx) = tokio::io::duplex(1024 * 1024);
    let reader_stream = ReaderStream::new(rx);
    let (ready_tx, ready_rx) = oneshot::channel::<Result<(), EngineError>>();
    let (done_tx, done_rx) = oneshot::channel::<Result<(), EngineError>>();
    let uq_engine = Arc::clone(&state.engine);
    // The first batch must also arrive within the maximum duration.
    let query_timeout = match (state.query_timeout, state.max_query_duration) {
        (Some(timeout), Some(max)) => Some(timeout.min(max)),
        (timeout, max) => timeout.or(max),
    };
    let (canceller_tx, canceller_rx) = oneshot::channel::<Arc<dyn QueryCanceller>>();
    let cancel_guard = CancelOnDrop(Some(canceller_rx));

    spawn_blocking(move || {
        // acquire a connection and defer SQL parsing to execute() — single prepare.
        let mut prepared = uq_engine.prepare(&sql).expect("pool acquire failed");
        // The handler already gave up (timeout or client disconnect) while this
        // task waited for a connection: release it without running the query.
        if canceller_tx.send(prepared.canceller()).is_err() {
            return;
        }

        // Execute. FirstBatchNotifier fires ready_tx on the first batch (or
        // finish for empty results). If execute() fails before any batch is
        // produced, ready_tx is still Some — we forward the error so the client
        // gets a 400 instead of a dangling request. Later errors go to done_tx,
        // which fails the response body.
        let bridge = SyncIoBridge::new(tx);
        macro_rules! stream_with_notifier {
            ($writer:expr) => {{
                let mut notifier = FirstBatchNotifier {
                    inner: $writer,
                    ready_tx: Some(ready_tx),
                };
                match prepared.execute(&mut notifier) {
                    Ok(()) => {
                        let _ = done_tx.send(Ok(()));
                    }
                    Err(e) => {
                        error!("execution failed: {}", e);
                        match notifier.ready_tx.take() {
                            Some(tx) => {
                                let _ = tx.send(Err(e));
                            }
                            None => {
                                let _ = done_tx.send(Err(e));
                            }
                        }
                    }
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

    // Timeout covers the time from request start until the first batch is ready;
    // a query still running at that point is cancelled by `cancel_guard`.
    // Once streaming begins, only the maximum duration applies.
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
    let canceller = cancel_guard.disarm();

    match ready_result {
        Ok(Ok(())) => {}
        Ok(Err(err @ EngineError::Forbidden(_))) => {
            return Err(UQueryError {
                status_code: StatusCode::FORBIDDEN.as_u16(),
                title: "Forbidden Query".to_string(),
                detail: err.to_string(),
            });
        }
        Ok(Err(err)) => {
            return Err(UQueryError {
                status_code: StatusCode::BAD_REQUEST.as_u16(),
                title: "SQL Error".to_string(),
                detail: err.to_string(),
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
        .body(response_body(
            reader_stream,
            done_rx,
            state.max_query_duration.map(|max| (start + max, canceller)),
        ))
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
    assert!(get_first_compatible_format(&headers).is_none());

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
