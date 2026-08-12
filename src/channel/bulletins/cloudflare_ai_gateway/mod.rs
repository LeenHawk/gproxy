//! Cloudflare AI Gateway channel using the recommended Cloudflare REST API.
//!
//! Requests target
//! `https://api.cloudflare.com/client/v4/accounts/{account_id}/ai/v1/...` and
//! select a gateway with `cf-aig-gateway-id`. The credential `api_key` is a
//! Cloudflare API token with Account > Workers AI > Read permission.

mod auth;

use http::HeaderName;

use crate::channel::bulletins::common::{self, ApiKeyDefaults};
use crate::channel::http_util::{allow_headers, allow_query, build_request};
use crate::channel::{Channel, ChannelError, PrepareCtx, PreparedRequest};

const DEFAULTS: ApiKeyDefaults = ApiKeyDefaults {
    default_base_url: Some("https://api.cloudflare.com"),
    forward_headers: &[
        "cf-aig-skip-cache",
        "cf-aig-cache-ttl",
        "cf-aig-cache-key",
        "cf-aig-collect-log",
        "cf-aig-request-timeout",
        "cf-aig-max-attempts",
        "cf-aig-retry-delay",
        "cf-aig-backoff",
        "cf-aig-metadata",
    ],
    forward_query: &[],
};

const DEFAULT_GATEWAY_ID: &str = "default";

pub struct CloudflareAiGatewayChannel;

fn secret<'a>(secret: &'a serde_json::Value, key: &'static str) -> Option<&'a str> {
    secret
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn upstream_path(ctx: &PrepareCtx<'_>) -> Result<String, ChannelError> {
    let account_id = secret(ctx.secret, "account_id")
        .ok_or_else(|| ChannelError::InvalidCredential("missing account_id".into()))?;
    Ok(format!(
        "/client/v4/accounts/{}/ai{}",
        crate::channel::oauth::percent_encode(account_id),
        ctx.path
    ))
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for CloudflareAiGatewayChannel {
    fn id(&self) -> &'static str {
        "cloudflare-ai-gateway"
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        use crate::channel::routes::{cg, local, pass, pv, xform};
        use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};
        vec![
            // The REST API does not expose a model-list endpoint.
            local(CountTokens, pv(P::OpenAi)),
            local(CountTokens, pv(P::Claude)),
            local(CountTokens, pv(P::Gemini)),
            pass(GenerateContent, cg(OpenAiResponses)),
            pass(GenerateContent, cg(OpenAiChatCompletions)),
            pass(GenerateContent, cg(ClaudeMessages)),
            xform(
                GenerateContent,
                cg(GeminiGenerateContent),
                GenerateContent,
                cg(OpenAiChatCompletions),
            ),
            pass(StreamGenerateContent, cg(OpenAiResponses)),
            pass(StreamGenerateContent, cg(OpenAiChatCompletions)),
            pass(StreamGenerateContent, cg(ClaudeMessages)),
            xform(
                StreamGenerateContent,
                cg(GeminiGenerateContent),
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
            ),
            xform(
                CompactContent,
                pv(P::OpenAi),
                GenerateContent,
                cg(OpenAiResponses),
            ),
        ]
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let path = upstream_path(&ctx)?;
        let api_token = common::resolve_api_key(&ctx)?;
        let gateway_id = secret(ctx.secret, "gateway_id")
            .unwrap_or(DEFAULT_GATEWAY_ID)
            .to_owned();
        let query = allow_query(ctx.query, DEFAULTS.forward_query);
        let uri = common::resolve_uri(&ctx, &DEFAULTS, &path, query.as_deref())?;
        let headers = allow_headers(ctx.headers, DEFAULTS.forward_headers);
        let mut req = build_request(ctx.method, uri, headers, ctx.body)?;
        auth::apply(&mut req, &api_token)?;
        common::inject_header(
            &mut req,
            HeaderName::from_static("cf-aig-gateway-id"),
            &gateway_id,
        )?;
        Ok(PreparedRequest::new(req))
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use http::{HeaderMap, HeaderValue, Method};
    use serde_json::json;

    use super::*;
    use crate::protocol::{ContentGenerationKind, Operation, OperationKey};

    fn prepare(
        settings: &serde_json::Value,
        secret: &serde_json::Value,
        headers: &HeaderMap,
        path: &str,
    ) -> Result<http::Request<Bytes>, ChannelError> {
        CloudflareAiGatewayChannel
            .prepare(PrepareCtx {
                secret,
                provider_settings: settings,
                op: OperationKey::content_generation(
                    Operation::GenerateContent,
                    ContentGenerationKind::OpenAiChatCompletions,
                ),
                stream: false,
                upstream_model_id: "openai/gpt-5-mini",
                method: Method::POST,
                path,
                query: None,
                headers,
                body: Bytes::from_static(b"{}"),
            })?
            .into_http()
            .map_err(|error| ChannelError::Build(error.to_string()))
    }

    #[test]
    fn builds_recommended_rest_endpoint_and_auth() {
        let settings = json!({});
        let secret = json!({
            "api_key": "cf-token",
            "account_id": "account-id",
            "gateway_id": "production"
        });
        let mut headers = HeaderMap::new();
        headers.insert("cf-aig-cache-ttl", HeaderValue::from_static("300"));
        headers.insert(
            "cf-aig-gateway-id",
            HeaderValue::from_static("untrusted-inbound"),
        );

        let req = prepare(&settings, &secret, &headers, "/v1/chat/completions").unwrap();
        assert_eq!(
            req.uri().to_string(),
            "https://api.cloudflare.com/client/v4/accounts/account-id/ai/v1/chat/completions"
        );
        assert_eq!(req.headers()["authorization"], "Bearer cf-token");
        assert_eq!(req.headers()["cf-aig-gateway-id"], "production");
        assert_eq!(req.headers()["cf-aig-cache-ttl"], "300");
    }

    #[test]
    fn defaults_gateway_and_requires_account() {
        let secret = json!({ "api_key": "cf-token", "account_id": "abc" });
        let headers = HeaderMap::new();
        let settings = json!({});
        let req = prepare(&settings, &secret, &headers, "/v1/responses").unwrap();
        assert_eq!(req.headers()["cf-aig-gateway-id"], "default");

        let missing = json!({ "api_key": "cf-token" });
        let err = prepare(&json!({}), &missing, &headers, "/v1/responses").unwrap_err();
        assert!(matches!(err, ChannelError::InvalidCredential(_)));
    }
}
