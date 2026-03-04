use crate::error::UQueryError;
use crate::web::request::QueryRequest;
use crate::{QueryResponseFormat, UQueryState};
use arrow::csv::Writer;
use arrow::ipc::writer::StreamWriter;
use arrow::json::{ArrayWriter, LineDelimitedWriter};
use arrow::record_batch::RecordBatchWriter;
use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::header::{ACCEPT, CONTENT_TYPE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::post;
use duckdb::Arrow;
use std::sync::Arc;
use tokio::task::spawn_blocking;
use tokio::time::Instant;
use tokio_util::io::{ReaderStream, SyncIoBridge};
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tracing::debug;

pub const CONTENT_TYPE_CSV: &str = "text/csv";
pub const CONTENT_TYPE_JSON: &str = "application/json";
pub const CONTENT_TYPE_JSONLINES: &str = "application/jsonlines";
pub const CONTENT_TYPE_JSONL: &str = "application/jsonl";
pub const CONTENT_TYPE_ARROW: &str = "application/vnd.apache.arrow.stream";
pub const CONTENT_TYPE_ANY: &str = "*/*";

pub fn create_router(state: Arc<UQueryState>, cors_enabled: bool) -> Router {
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
    State(state): State<Arc<UQueryState>>,
    headers: HeaderMap,
    query_request: QueryRequest,
) -> pingora::Result<Response, UQueryError> {
    let format = get_first_compatible_format(&headers).ok_or_else(|| UQueryError {
        status_code: StatusCode::NOT_ACCEPTABLE,
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
    let (tx, rx) = tokio::io::duplex(65_536);
    let reader_stream = ReaderStream::new(rx);
    let (result_sender, result_receiver) = tokio::sync::oneshot::channel();

    spawn_blocking(move || {
        let bridge = SyncIoBridge::new(tx);
        let query_start = Instant::now();
        let conn = state.get_new_connection();

        let statement = conn.prepare(query_request.get_sql_query());
        match statement {
            Ok(mut statement) => match statement.query_arrow([]) {
                Ok(arrow) => {
                    debug!(
                        "run: [{}] in {:?}",
                        query_request.get_sql_query(),
                        query_start.elapsed()
                    );
                    let _ = result_sender.send(Ok::<(), String>(()));
                    match format {
                        QueryResponseFormat::Csv => {
                            let writer = Writer::new(bridge);
                            handle_response_write(writer, arrow);
                        }
                        QueryResponseFormat::Json => {
                            let writer = ArrayWriter::new(bridge);
                            handle_response_write(writer, arrow);
                        }
                        QueryResponseFormat::Arrow => {
                            let writer =
                                StreamWriter::try_new(bridge, &arrow.get_schema()).unwrap();
                            handle_response_write(writer, arrow);
                        }
                        QueryResponseFormat::JsonLINES => {
                            let writer = LineDelimitedWriter::new(bridge);
                            handle_response_write(writer, arrow);
                        }
                    };
                }
                Err(err) => {
                    let _ = result_sender.send(Err(err.to_string()));
                }
            },
            Err(err) => {
                let _ = result_sender.send(Err(err.to_string()));
            }
        }
    });

    let result = result_receiver.await.unwrap();
    match result {
        Ok(_) => Ok(axum::response::Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, content_type)
            .body(Body::from_stream(reader_stream))
            .unwrap()),
        Err(err) => Err(UQueryError {
            status_code: StatusCode::BAD_REQUEST,
            title: "SQL Error".to_string(),
            detail: err,
        }),
    }
}

fn handle_response_write<W: RecordBatchWriter>(mut writer: W, data: Arrow) {
    for rb in data {
        writer.write(&rb).unwrap();
    }
    writer.close().unwrap();
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
