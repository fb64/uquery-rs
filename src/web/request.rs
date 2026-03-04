use crate::error::UQueryError;
use crate::web::routers::CONTENT_TYPE_JSON;
use axum::body::Body;
use axum::extract::FromRequest;
use axum::http::header::CONTENT_TYPE;
use axum::http::{Request, StatusCode};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct QueryRequest {
    sql_query: String,
}

impl QueryRequest {
    pub fn new(query: String) -> Self {
        Self { sql_query: query }
    }
    pub fn get_sql_query(&self) -> &str {
        &self.sql_query
    }
}

impl<S> FromRequest<S> for QueryRequest
where
    S: Send + Sync,
{
    type Rejection = UQueryError;

    async fn from_request(
        req: Request<Body>,
        _state: &S,
    ) -> pingora::Result<Self, Self::Rejection> {
        let (parts, body) = req.into_parts();

        let content_type = parts
            .headers
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        let bytes = axum::body::to_bytes(body, usize::MAX)
            .await
            .map_err(|e| UQueryError {
                status_code: StatusCode::BAD_REQUEST,
                title: "Failed to read request body".to_string(),
                detail: e.to_string(),
            })?;

        if content_type.contains(CONTENT_TYPE_JSON) {
            let payload: QueryRequest =
                serde_json::from_slice(&bytes).map_err(|e| UQueryError {
                    status_code: StatusCode::BAD_REQUEST,
                    title: "Invalid JSON".to_string(),
                    detail: e.to_string(),
                })?;
            Ok(payload)
        } else {
            // text/plain or any other - treat as raw SQL
            let sql = String::from_utf8(bytes.to_vec()).map_err(|e| UQueryError {
                status_code: StatusCode::BAD_REQUEST,
                title: "Invalid UTF-8".to_string(),
                detail: e.to_string(),
            })?;
            Ok(QueryRequest::new(sql))
        }
    }
}
