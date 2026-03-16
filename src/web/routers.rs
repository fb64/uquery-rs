use crate::core::engine::UQueryEngine;
use crate::core::error::UQueryError;
use crate::web::consumers::{ArrowConsumer, WriterConsumer};
use crate::web::request::QueryRequest;
use crate::web::response::QueryResponseFormat;
use crate::web::{
    CONTENT_TYPE_ANY, CONTENT_TYPE_ARROW, CONTENT_TYPE_CSV, CONTENT_TYPE_JSON, CONTENT_TYPE_JSONL,
    CONTENT_TYPE_JSONLINES,
};
use arrow::csv::Writer as CsvWriter;
use arrow::json::{ArrayWriter, LineDelimitedWriter};

use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::header::{ACCEPT, CONTENT_TYPE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::post;
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::task::spawn_blocking;
use tokio_util::io::{ReaderStream, SyncIoBridge};
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tracing::error;

pub fn create_router(state: Arc<dyn UQueryEngine>, cors_enabled: bool) -> Router {
    let router = Router::new()
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
    State(uq_engine): State<Arc<dyn UQueryEngine>>,
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
    let (tx, rx) = tokio::io::duplex(65_536);
    let reader_stream = ReaderStream::new(rx);
    let (ready_tx, ready_rx) = oneshot::channel::<Result<(), String>>();

    spawn_blocking(move || {
        // Phase 1: validate — if the SQL is invalid, signal the error and fail.
        let mut prepared = match uq_engine.prepare(&sql) {
            Err(e) => {
                let _ = ready_tx.send(Err(e));
                return;
            }
            Ok(q) => q,
        };

        // Phase 2: query is valid — let the handler send the 200 and start
        // draining the pipe before any batch is written, avoiding a deadlock
        // when the response exceeds the duplex buffer size.
        let _ = ready_tx.send(Ok(()));

        // Phase 3: stream batches into the pipe.
        let bridge = SyncIoBridge::new(tx);
        match format {
            QueryResponseFormat::Csv => {
                if let Err(e) = prepared.execute(&mut WriterConsumer::new(CsvWriter::new(bridge))) {
                    error!("CSV execution failed: {}", e);
                }
            }
            QueryResponseFormat::Json => {
                if let Err(e) = prepared.execute(&mut WriterConsumer::new(ArrayWriter::new(bridge)))
                {
                    error!("Json execution failed: {}", e);
                }
            }
            QueryResponseFormat::Arrow => {
                if let Err(e) = prepared.execute(&mut ArrowConsumer::new(bridge)) {
                    error!("Arrow execution failed: {}", e);
                }
            }
            QueryResponseFormat::JsonLINES => {
                if let Err(e) =
                    prepared.execute(&mut WriterConsumer::new(LineDelimitedWriter::new(bridge)))
                {
                    error!("JsonLines execution failed: {}", e);
                }
            }
        }
    });

    match ready_rx.await.unwrap() {
        Ok(()) => Ok(Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, content_type)
            .body(Body::from_stream(reader_stream))
            .unwrap()),
        Err(err) => Err(UQueryError {
            status_code: StatusCode::BAD_REQUEST.as_u16(),
            title: "SQL Error".to_string(),
            detail: err,
        }),
    }
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
