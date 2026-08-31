//! Shared URL and ingress pass-through helpers for Realtime WebSockets.

use bytes::Bytes;
use http::{Method, Request};

use crate::channel::bulletins::common::{self, ApiKeyDefaults};
use crate::channel::http_util::{
    allow_headers_with_settings, build_request, passthrough_query_with_settings,
};
use crate::channel::{ChannelError, PrepareCtx};

pub fn is_target(method: &Method, path: &str) -> bool {
    *method == Method::GET && matches!(path, "/v1/realtime" | "/v1/live")
}

pub fn is_ingress_path(path: &str) -> bool {
    if matches!(path, "/v1/realtime" | "/v1/live") {
        return true;
    }
    let Some((provider, rest)) = path.trim_start_matches('/').split_once('/') else {
        return false;
    };
    !provider.is_empty()
        && !matches!(provider, "v1" | "v1beta" | "console")
        && matches!(rest, "v1/realtime" | "v1/live")
}

/// Build an OpenAI API-key request while retaining the Realtime query surface.
pub fn build_api_key_request(
    ctx: PrepareCtx<'_>,
    defaults: &ApiKeyDefaults,
    forward_headers: &[&str],
) -> Result<(Request<Bytes>, String), ChannelError> {
    let key = common::resolve_api_key(&ctx)?;
    let sanitized_query = sanitize_query(ctx.query);
    let query = passthrough_query_with_settings(sanitized_query.as_deref(), ctx.provider_settings);
    let uri = common::resolve_uri(&ctx, defaults, ctx.path, query.as_deref())?;
    let headers = allow_headers_with_settings(ctx.headers, forward_headers, ctx.provider_settings);
    let request = build_request(ctx.method, uri, headers, ctx.body)?;
    Ok((request, key))
}

pub fn query_model(query: Option<&str>) -> Option<String> {
    serde_urlencoded::from_str::<Vec<(String, String)>>(query?)
        .ok()?
        .into_iter()
        .find_map(|(key, value)| (key == "model" && !value.is_empty()).then_some(value))
}

/// Replace model values without normalizing any unrelated query pair.
pub fn rewrite_model_query(query: Option<&str>, model: &str) -> Result<String, ChannelError> {
    let query = query.ok_or_else(|| ChannelError::Build("realtime query missing model".into()))?;
    let replacement = serde_urlencoded::to_string([("model", model)])
        .map_err(|error| ChannelError::Build(format!("bad realtime model query: {error}")))?;
    let mut found = false;
    let rewritten = query
        .split('&')
        .map(|pair| {
            if pair.split('=').next() == Some("model") {
                found = true;
                replacement.as_str()
            } else {
                pair
            }
        })
        .collect::<Vec<_>>()
        .join("&");
    if found {
        Ok(rewritten)
    } else if query_value(query, "call_id").is_some() {
        // A WebRTC sideband joins an already-created call and normally carries
        // only `call_id`; its model is recovered from the gproxy call binding.
        Ok(query.to_owned())
    } else {
        Err(ChannelError::Build("realtime query missing model".into()))
    }
}

fn query_value(query: &str, name: &str) -> Option<String> {
    serde_urlencoded::from_str::<Vec<(String, String)>>(query)
        .ok()?
        .into_iter()
        .find_map(|(key, value)| (key == name && !value.is_empty()).then_some(value))
}

/// Remove downstream API-key parameters while preserving every other raw pair.
pub fn sanitize_query(query: Option<&str>) -> Option<String> {
    let kept = query?
        .split('&')
        .filter(|pair| pair.split('=').next() != Some("key"))
        .collect::<Vec<_>>();
    (!kept.is_empty()).then(|| kept.join("&"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderMap;
    use serde_json::json;

    use crate::protocol::{Operation, OperationKey, Provider};

    #[test]
    fn model_rewrite_preserves_other_query_pairs() {
        let query = rewrite_model_query(
            Some("model=public&intent=quicksilver&flag=&thread=abc%2Fdef"),
            "gpt-realtime/preview",
        )
        .unwrap();
        assert_eq!(
            query,
            "model=gpt-realtime%2Fpreview&intent=quicksilver&flag=&thread=abc%2Fdef"
        );
        assert_eq!(
            query_model(Some(&query)).as_deref(),
            Some("gpt-realtime/preview")
        );
    }

    #[test]
    fn exact_openai_endpoint_merges_passthrough_query() {
        let secret = json!({ "api_key": "sk-test" });
        let settings = json!({
            "endpoints": { "openai_realtime": "https://relay.example/socket?fixed=1" }
        });
        let headers = HeaderMap::new();
        let defaults = ApiKeyDefaults {
            default_base_url: Some("https://api.openai.com"),
            forward_headers: &[],
            forward_query: &[],
        };
        let (request, _) = build_api_key_request(
            PrepareCtx {
                secret: &secret,
                provider_settings: &settings,
                op: OperationKey::provider(Operation::ConnectRealtime, Provider::OpenAi),
                stream: true,
                upstream_model_id: "gpt-realtime",
                method: Method::GET,
                path: "/v1/realtime",
                query: Some("key=gproxy-secret&model=gpt-realtime&intent=quicksilver"),
                headers: &headers,
                body: Bytes::new(),
            },
            &defaults,
            &[],
        )
        .unwrap();
        let uri = crate::channel::responses_websocket::websocket_uri(request.uri()).unwrap();
        assert_eq!(
            uri,
            "wss://relay.example/socket?fixed=1&model=gpt-realtime&intent=quicksilver"
        );
    }
}
