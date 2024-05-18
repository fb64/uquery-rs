use axum::{Json, Router};
use axum::body::Body;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use duckdb::Connection;
use polars::prelude::DataFrame;
use polars_core::utils::accumulate_dataframes_vertical_unchecked;
use polars_io::json::JsonWriter;
use polars_io::SerWriter;
use serde::Deserialize;
use tokio::task::spawn_blocking;
use tokio_util::io::{ReaderStream, SyncIoBridge};

#[derive(Deserialize)]
struct QueryRequest{
    query: String
}

struct QueryResponse{
    data: DataFrame
}

impl IntoResponse for QueryResponse{
    fn into_response(mut self) -> Response {

        let (tx, rx) = tokio::io::duplex(65_536);
        let reader_stream = ReaderStream::new(rx);
        spawn_blocking(move || {
            let bridge = SyncIoBridge::new(tx);
            JsonWriter::new(bridge).finish(&mut self.data).unwrap()
        });

        axum::response::Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .body(Body::from_stream(reader_stream))
            .unwrap()
    }
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(listener, app()).await.unwrap();
}

fn app() -> Router{
    Router::new().route("/", post(query))
}

async fn query(Json(payload): Json<QueryRequest>) -> Result<QueryResponse,StatusCode>{

    let response = handle_query(payload.query);
    match response {
        Ok(df) => {Ok(QueryResponse{data:df})}
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
    use std::str::from_utf8;
    use axum::body::Body;
    use axum::http;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;
    use crate::{app};
    use futures_util::StreamExt;

    #[tokio::test]
    async fn query_test() {
        let app = app();
        //let request = QueryRequest{query:String::from("SELECT * FROM (VALUES (1,'Rust'), (2,'Java'), (3,'Python')) Language(Id,Name)")};
        let response = app
            .oneshot(
                Request::builder()
                    .method(http::Method::POST)
                    .header("Content-Type","application/json")
                    .uri("/")
                    .body(Body::from(String::from("{\"query\":\"SELECT * FROM (VALUES (1,'Rust','Safe, concurrent, performant systems language')) Language(Id,Name,Description)\"}"))).unwrap()
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
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

        assert_eq!(from_utf8(&*result).unwrap(),"{\"Id\":1,\"Name\":\"Rust\",\"Description\":\"Safe, concurrent, performant systems language\"}\n");
    }
}
