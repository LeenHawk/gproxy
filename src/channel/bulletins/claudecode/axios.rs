//! Axios 1.13.6 Node-adapter wire shape used by Claude Code OAuth helpers.

use bytes::Bytes;
use http::header::{ACCEPT, ACCEPT_ENCODING, USER_AGENT};
use http::{HeaderValue, Request};

use crate::http::client::TransportOptions;

pub(super) fn apply(request: &mut Request<Bytes>, timeout_secs: u64, omit_body: bool) {
    let headers = request.headers_mut();
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/json, text/plain, */*"),
    );
    headers.insert(
        ACCEPT_ENCODING,
        HeaderValue::from_static("gzip, compress, deflate, br"),
    );
    headers.insert(USER_AGENT, HeaderValue::from_static("axios/1.13.6"));
    request.extensions_mut().insert(TransportOptions {
        total_timeout: Some(std::time::Duration::from_secs(timeout_secs)),
        max_redirects: Some(21),
        http_version: Some(http::Version::HTTP_11),
        omit_body,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_node_adapter_shape() {
        let mut request = Request::get("https://api.anthropic.com/api/oauth/profile")
            .body(Bytes::new())
            .unwrap();
        apply(&mut request, 10, true);

        assert_eq!(request.headers()[USER_AGENT], "axios/1.13.6");
        assert_eq!(
            request.headers()[ACCEPT_ENCODING],
            "gzip, compress, deflate, br"
        );
        let options = request.extensions().get::<TransportOptions>().unwrap();
        assert_eq!(
            options.total_timeout,
            Some(std::time::Duration::from_secs(10))
        );
        assert!(options.omit_body);
    }
}
