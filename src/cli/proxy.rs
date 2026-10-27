use async_trait::async_trait;
use pingora::http::RequestHeader;
use pingora::prelude::{HttpPeer, ProxyHttp, Session};

pub struct UIProxyService;

#[async_trait]
impl ProxyHttp for UIProxyService {
    type CTX = ();
    fn new_ctx(&self) -> Self::CTX {}

    async fn upstream_peer(
        &self,
        _session: &mut Session,
        _ctx: &mut Self::CTX,
    ) -> pingora::Result<Box<HttpPeer>> {
        let upstream = Box::new(HttpPeer::new(
            "localhost:4213",
            false,
            "localhost".to_string(),
        ));
        Ok(upstream)
    }

    async fn upstream_request_filter(
        &self,
        _session: &mut Session,
        upstream_request: &mut RequestHeader,
        _ctx: &mut Self::CTX,
    ) -> pingora::Result<()> {
        upstream_request.insert_header("Host", "localhost").unwrap();
        upstream_request
            .insert_header("Referer", "http://localhost:4213/")
            .unwrap();
        upstream_request
            .insert_header("Origin", "http://localhost:4213")
            .unwrap();
        Ok(())
    }
}
