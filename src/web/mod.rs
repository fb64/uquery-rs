pub const CONTENT_TYPE_CSV: &str = "text/csv";
pub const CONTENT_TYPE_JSON: &str = "application/json";
pub const CONTENT_TYPE_JSONLINES: &str = "application/jsonlines";
pub const CONTENT_TYPE_JSONL: &str = "application/jsonl";
pub const CONTENT_TYPE_ARROW: &str = "application/vnd.apache.arrow.stream";
pub const CONTENT_TYPE_ANY: &str = "*/*";

pub mod proxy;
pub mod request;
pub mod response;
pub mod routers;
