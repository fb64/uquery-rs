use crate::core::error::UQueryError;
use crate::web::{CONTENT_TYPE_ARROW, CONTENT_TYPE_CSV, CONTENT_TYPE_JSON, CONTENT_TYPE_JSONLINES};
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use std::fmt::Display;

pub(crate) enum QueryResponseFormat {
    Csv,
    Json,
    Arrow,
    JsonLINES,
}

impl Display for QueryResponseFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            QueryResponseFormat::Csv => CONTENT_TYPE_CSV.to_string(),
            QueryResponseFormat::Json => CONTENT_TYPE_JSON.to_string(),
            QueryResponseFormat::Arrow => CONTENT_TYPE_ARROW.to_string(),
            QueryResponseFormat::JsonLINES => CONTENT_TYPE_JSONLINES.to_string(),
        };
        write!(f, "{}", str)
    }
}

impl IntoResponse for UQueryError {
    fn into_response(self) -> Response {
        let mut response = (
            StatusCode::from_u16(self.status_code).unwrap(),
            serde_json::to_string(&self).unwrap(),
        )
            .into_response();

        response
            .headers_mut()
            .insert(CONTENT_TYPE, "application/problem+json".parse().unwrap());
        response
    }
}
