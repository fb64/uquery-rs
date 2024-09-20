use axum::http::header::CONTENT_TYPE;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

#[derive(Debug, Clone)]
pub struct UQueryError {
    pub(crate) status_code: StatusCode,
    pub(crate) title: String,
    pub(crate) detail: String,
}

impl IntoResponse for UQueryError {
    fn into_response(self) -> Response {
        let mut response = (self.status_code, serde_json::to_string(&self).unwrap()).into_response();

        response
            .headers_mut()
            .insert(CONTENT_TYPE, "application/problem+json".parse().unwrap());
        response
    }
}

impl Serialize for UQueryError{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer
    {
        let mut state = serializer.serialize_struct("UQueryError", 3)?;
        state.serialize_field("status", &self.status_code.as_u16())?;
        state.serialize_field("title", &self.title)?;
        state.serialize_field("detail", &self.detail)?;
        state.end()
    }
}
