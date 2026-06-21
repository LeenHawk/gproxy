//! Inbound request → [`RequestCtx`] extraction (request-id, routing mode, path
//! normalization). Body reading + the body-size limit live in the gateway
//! (the shared cap is [`crate::config::MAX_BODY_BYTES`]).

use bytes::Bytes;
use http::request::Parts;

use crate::pipeline::context::{RequestCtx, RoutingMode};
use crate::pipeline::error::PipelineError;

/// Build a [`RequestCtx`] from request parts + the already-read body. For scoped
/// mode the leading `/{provider}` segment is stripped so `path` is `/v1/...` in
/// both modes.
pub fn build_ctx(parts: Parts, body: Bytes, scoped: bool) -> Result<RequestCtx, PipelineError> {
    let query = parts.uri.query().map(|q| q.to_string());
    let raw_path = parts.uri.path();

    let (mode, path) = if scoped {
        let trimmed = raw_path.trim_start_matches('/');
        let (provider, rest) = trimmed
            .split_once('/')
            .ok_or(PipelineError::UnsupportedPath)?;
        if provider.is_empty() || provider == "v1" || provider == "v1beta" || provider == "console"
        {
            return Err(PipelineError::UnsupportedPath);
        }
        (
            RoutingMode::Scoped {
                provider: provider.to_string(),
            },
            format!("/{rest}"),
        )
    } else {
        (RoutingMode::Aggregated, raw_path.to_string())
    };

    Ok(RequestCtx {
        request_id: gen_request_id(),
        method: parts.method,
        path,
        query,
        headers: parts.headers,
        body,
        mode,
        identity: None,
        op: None,
        stream: false,
        route_name: None,
        pending_micros: 0,
    })
}

/// Per-request correlation id (§15.1): a ULID — lexicographically sortable by
/// creation time, unique + opaque on both native and edge.
fn gen_request_id() -> String {
    crate::util::id::ulid()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::context::RoutingMode;

    fn parts(path: &str) -> Parts {
        http::Request::builder()
            .uri(path)
            .body(())
            .unwrap()
            .into_parts()
            .0
    }

    /// Gemini speaks `/v1beta/...`: scoped mode strips the provider but keeps the
    /// `/v1beta/...` remainder intact for `classify` (regression for the gemini
    /// 404 where the surface was unrouted).
    #[test]
    fn scoped_v1beta_strips_provider_keeps_path() {
        let ctx = build_ctx(
            parts("/geminicli/v1beta/models/m:generateContent"),
            Bytes::new(),
            true,
        )
        .unwrap();
        assert!(matches!(ctx.mode, RoutingMode::Scoped { provider } if provider == "geminicli"));
        assert_eq!(ctx.path, "/v1beta/models/m:generateContent");
    }

    #[test]
    fn aggregated_v1beta_path_preserved() {
        let ctx = build_ctx(parts("/v1beta/models"), Bytes::new(), false).unwrap();
        assert!(matches!(ctx.mode, RoutingMode::Aggregated));
        assert_eq!(ctx.path, "/v1beta/models");
    }

    /// `v1beta` is a reserved first segment, never a provider name.
    #[test]
    fn bare_v1beta_provider_rejected() {
        assert!(build_ctx(parts("/v1beta/models"), Bytes::new(), true).is_err());
    }
}
