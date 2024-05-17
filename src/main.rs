use axum::Router;
use axum::routing::get;

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(|| async { "Work in progress" }));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
