use axum::{Json, Router};
use axum::body::Body;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::Deserialize;

#[derive(Deserialize)]
struct QueryRequest{
    query: String
}

struct QueryResponse{
    data: String
}

impl IntoResponse for QueryResponse{
    fn into_response(self) -> Response {
        axum::response::Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/plain")
            .body(Body::from(self.data))
            .unwrap()
    }
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", post(query));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn query(Json(payload): Json<QueryRequest>) -> QueryResponse{
    QueryResponse{data:format!("[WIP] - query: {}",payload.query)}
}
