//! Outbound HTTP implementations for the host-owned channel transport.

pub use crate::channel::transport::{
    ByteStreamDecoder, ClientError, RespStream, TransportOptions, UpstreamClient,
};
#[cfg(not(target_arch = "wasm32"))]
pub use crate::channel::transport::{ConduitFrame, ConduitSocket};

#[cfg(any(target_arch = "wasm32", test))]
fn validate_fetch_options(options: Option<&TransportOptions>) -> Result<(), ClientError> {
    let Some(options) = options else {
        return Ok(());
    };
    if options.max_redirects.is_some_and(|limit| limit > 0) {
        return Err(ClientError::Config(
            "Fetch cannot enforce a positive max_redirects bound; use None or Some(0)".into(),
        ));
    }
    if options.http_version.is_some() {
        return Err(ClientError::Config(
            "Fetch cannot select an HTTP version; use None".into(),
        ));
    }
    Ok(())
}

#[cfg(any(target_arch = "wasm32", test))]
fn fetch_sends_body(method: &http::Method, options: Option<&TransportOptions>) -> bool {
    *method != http::Method::GET
        && *method != http::Method::HEAD
        && !options.is_some_and(|options| options.omit_body)
}

#[cfg(any(target_arch = "wasm32", test))]
fn fetch_websocket_frame(
    body: bytes::Bytes,
    options: Option<&TransportOptions>,
) -> Result<Option<String>, ClientError> {
    if options.is_some_and(|options| options.omit_body) {
        return Ok(None);
    }
    String::from_utf8(body.to_vec()).map(Some).map_err(|error| {
        ClientError::Transport(format!(
            "responses websocket request is not UTF-8 JSON: {error}"
        ))
    })
}

#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
mod fingerprint;
#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
mod pool;
#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
pub use pool::ClientPool;

#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
mod proxy_url;
#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
mod wreq;
#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
pub use wreq::WreqClient;

#[cfg(all(target_arch = "wasm32", feature = "upstream-fetch"))]
mod fetch;
#[cfg(all(target_arch = "wasm32", feature = "upstream-fetch"))]
pub use fetch::FetchClient;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_transport_policy_is_explicit_and_honors_body_omission() {
        let no_redirect = TransportOptions {
            max_redirects: Some(0),
            omit_body: true,
            ..Default::default()
        };
        assert!(validate_fetch_options(Some(&no_redirect)).is_ok());
        assert!(!fetch_sends_body(&http::Method::POST, Some(&no_redirect)));
        assert!(!fetch_sends_body(&http::Method::GET, None));
        assert!(fetch_sends_body(&http::Method::POST, None));
        assert_eq!(
            fetch_websocket_frame(bytes::Bytes::from_static(b"frame"), Some(&no_redirect)).unwrap(),
            None
        );
        assert_eq!(
            fetch_websocket_frame(bytes::Bytes::from_static(b"frame"), None).unwrap(),
            Some("frame".into())
        );

        let bounded = TransportOptions {
            max_redirects: Some(1),
            ..Default::default()
        };
        assert!(matches!(
            validate_fetch_options(Some(&bounded)),
            Err(ClientError::Config(_))
        ));
        let versioned = TransportOptions {
            http_version: Some(http::Version::HTTP_11),
            ..Default::default()
        };
        assert!(matches!(
            validate_fetch_options(Some(&versioned)),
            Err(ClientError::Config(_))
        ));
    }
}
