use crate::core::error::UQueryError;
use axum::extract::{Request, State};
use axum::http::header::{AUTHORIZATION, ORIGIN, WWW_AUTHENTICATE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;
use subtle::ConstantTimeEq;

pub const X_API_KEY: &str = "x-api-key";

/// Generate a random 256-bit API key, hex encoded.
pub fn generate_api_key() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("failed to read random bytes");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Reject requests without a valid API key in `Authorization: Bearer <key>` or
/// `X-API-Key: <key>`.
pub async fn require_api_key(
    State(api_key): State<Arc<str>>,
    request: Request,
    next: Next,
) -> Response {
    if request_keys(request.headers()).any(|key| key.as_bytes().ct_eq(api_key.as_bytes()).into()) {
        return next.run(request).await;
    }
    let mut response = UQueryError {
        status_code: StatusCode::UNAUTHORIZED.as_u16(),
        title: "Unauthorized".to_string(),
        detail: "missing or invalid API key".to_string(),
    }
    .into_response();
    response
        .headers_mut()
        .insert(WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
    response
}

/// Reject requests sent by a web browser, recognized by their `Origin` header.
///
/// Browsers send cross-origin `text/plain` and form POSTs without a CORS preflight,
/// so without this check any web page could run queries on a server its visitor
/// can reach. Browsers send `Origin` on every POST, which also covers DNS
/// rebinding. Used when CORS is disabled.
pub async fn reject_browser_requests(request: Request, next: Next) -> Response {
    if !request.headers().contains_key(ORIGIN) {
        return next.run(request).await;
    }
    UQueryError {
        status_code: StatusCode::FORBIDDEN.as_u16(),
        title: "Forbidden".to_string(),
        detail:
            "requests from web browsers (with an Origin header) are rejected unless CORS is enabled"
                .to_string(),
    }
    .into_response()
}

fn request_keys(headers: &HeaderMap) -> impl Iterator<Item = &str> {
    let bearer = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        .map(|(_, key)| key.trim());
    let x_api_key = headers.get(X_API_KEY).and_then(|value| value.to_str().ok());
    bearer.into_iter().chain(x_api_key)
}

#[test]
fn generate_api_key_test() {
    let key = generate_api_key();
    assert_eq!(key.len(), 64);
    assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(key, generate_api_key());
}

#[test]
fn request_keys_test() {
    let mut headers = HeaderMap::new();
    assert_eq!(request_keys(&headers).count(), 0);
    headers.insert(AUTHORIZATION, HeaderValue::from_static("Basic abc"));
    assert_eq!(request_keys(&headers).count(), 0);
    headers.insert(AUTHORIZATION, HeaderValue::from_static("bearer abc"));
    headers.insert(X_API_KEY, HeaderValue::from_static("xyz"));
    assert_eq!(
        request_keys(&headers).collect::<Vec<_>>(),
        vec!["abc", "xyz"]
    );
}
