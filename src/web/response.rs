use crate::core::error::UQueryError;
use crate::web::{
    CONTENT_TYPE_ARROW, CONTENT_TYPE_ARROW_WITH_CODECS, CONTENT_TYPE_CSV, CONTENT_TYPE_JSON,
    CONTENT_TYPE_JSONLINES,
};
use arrow::ipc::CompressionType;
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use std::fmt::Display;

/// Arrow IPC buffer compression codec, negotiated through the `codecs` parameter of the
/// Arrow media type in the `Accept` header (e.g. `codecs="zstd, lz4"`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ArrowCodec {
    Zstd,
    Lz4,
}

impl ArrowCodec {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "zstd" => Some(ArrowCodec::Zstd),
            "lz4" => Some(ArrowCodec::Lz4),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ArrowCodec::Zstd => "zstd",
            ArrowCodec::Lz4 => "lz4",
        }
    }
}

impl From<ArrowCodec> for CompressionType {
    fn from(codec: ArrowCodec) -> Self {
        match codec {
            ArrowCodec::Zstd => CompressionType::ZSTD,
            ArrowCodec::Lz4 => CompressionType::LZ4_FRAME,
        }
    }
}

#[derive(Debug, PartialEq)]
pub(crate) enum QueryResponseFormat {
    Csv,
    Json,
    Arrow(Option<ArrowCodec>),
    JsonLINES,
}

impl Display for QueryResponseFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            QueryResponseFormat::Csv => CONTENT_TYPE_CSV.to_string(),
            QueryResponseFormat::Json => CONTENT_TYPE_JSON.to_string(),
            QueryResponseFormat::Arrow(None) => CONTENT_TYPE_ARROW.to_string(),
            QueryResponseFormat::Arrow(Some(codec)) => {
                format!("{CONTENT_TYPE_ARROW_WITH_CODECS}{}", codec.name())
            }
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
