//! GitHub Copilot channel: a long-lived GitHub token is re-exchanged for a
//! short-lived Copilot token ([`auth`]), which authorizes an OpenAI
//! chat-completions passthrough against the account-typed Copilot host.
//!
//! There is no `refresh_token` — `needs_refresh` keys off the cached Copilot
//! token's expiry and `refresh` always re-exchanges from the GitHub token. The
//! request is plain OpenAI chat completions (the native format stays
//! `OpenAiChatCompletions`): NO envelope, NO stream decoder, NO normalize, body
//! verbatim. Login is the GitHub device flow ([`auth::device_start`] /
//! [`auth::device_poll`]).

mod auth;
#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
mod fingerprint;
mod usage;
use std::sync::Arc;

use bytes::Bytes;
use serde_json::Value;

use crate::channel::http_util::{allow_headers, build_request, join_url};
use crate::channel::{
    Channel, ChannelError, ChannelLogin, DeviceInit, DevicePoll, PrepareCtx, PreparedRequest,
};
use crate::http::client::UpstreamClient;
use crate::protocol::Provider;

/// Re-exchange the Copilot token slightly before it expires to avoid racing a
/// 401 mid-flight.
const EXPIRY_SKEW_MS: i64 = 60_000;

pub struct CopilotCliChannel;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for CopilotCliChannel {
    fn id(&self) -> &'static str {
        "copilotcli"
    }

    fn provider_family(&self) -> Provider {
        Provider::OpenAi
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        use crate::channel::routes::{cg, local, pass, pv, xform};
        use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};
        vec![
            pass(ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Claude), ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Gemini), ListModels, pv(P::OpenAi)),
            pass(GetModel, pv(P::OpenAi)),
            xform(GetModel, pv(P::Claude), GetModel, pv(P::OpenAi)),
            xform(GetModel, pv(P::Gemini), GetModel, pv(P::OpenAi)),
            // Copilot has no upstream token-count endpoint (only /chat/completions
            // + /models); count locally like every other chat-only channel.
            local(CountTokens, pv(P::OpenAi)),
            local(CountTokens, pv(P::Claude)),
            local(CountTokens, pv(P::Gemini)),
            pass(GenerateContent, cg(OpenAiChatCompletions)),
            xform(
                GenerateContent,
                cg(OpenAiResponses),
                GenerateContent,
                cg(OpenAiChatCompletions),
            ),
            xform(
                GenerateContent,
                cg(ClaudeMessages),
                GenerateContent,
                cg(OpenAiChatCompletions),
            ),
            xform(
                GenerateContent,
                cg(GeminiGenerateContent),
                GenerateContent,
                cg(OpenAiChatCompletions),
            ),
            pass(StreamGenerateContent, cg(OpenAiChatCompletions)),
            xform(
                StreamGenerateContent,
                cg(OpenAiResponses),
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
            ),
            xform(
                StreamGenerateContent,
                cg(ClaudeMessages),
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
            ),
            xform(
                StreamGenerateContent,
                cg(GeminiGenerateContent),
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
            ),
            pass(CompactContent, pv(P::OpenAi)),
        ]
    }

    #[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
    fn default_emulation(&self) -> Option<wreq::Emulation> {
        Some(fingerprint::default_emulation())
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let copilot_token = ctx
            .secret
            .get("copilot_token")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ChannelError::InvalidCredential("missing copilot_token".into()))?;
        let machine_id = auth::machine_id(ctx.secret);
        let base = auth::base_url(ctx.secret);

        // Copilot exposes the catalogue at `GET {base}/models`; everything else is
        // the `POST {base}/chat/completions` passthrough. Without this, a model
        // pull (a GET) would be forced onto `/chat/completions` → upstream 405.
        let upstream_path = if ctx.method == http::Method::GET
            && ctx.path.trim_end_matches('/').ends_with("/models")
        {
            "/models"
        } else {
            "/chat/completions"
        };

        let uri = join_url(&base, upstream_path, None)?;
        // Copilot injects its own auth + editor headers; no inbound forwards.
        let headers = allow_headers(ctx.headers, &[]);
        let mut req = build_request(ctx.method, uri, headers, ctx.body)?;
        auth::apply_chat_headers(&mut req, copilot_token, &machine_id)?;
        Ok(PreparedRequest::new(req))
    }

    fn needs_refresh(&self, secret: &Value) -> bool {
        let cached = secret
            .get("copilot_token")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());
        if cached.is_none() {
            return true;
        }
        let expires_at_ms = secret
            .get("copilot_expires_at_ms")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let now_ms = crate::util::time::unix_now().saturating_mul(1000);
        now_ms > expires_at_ms - EXPIRY_SKEW_MS
    }

    async fn refresh(
        &self,
        client: &Arc<dyn UpstreamClient>,
        secret: &Value,
    ) -> Result<Value, ChannelError> {
        let github_token = auth::github_token(secret)?;
        let vscode_version = auth::vscode_version(secret);
        let resp = auth::exchange_copilot_token(client, github_token, vscode_version).await?;
        let expires_at_ms = resp.expires_at.saturating_mul(1000);

        // Preserve github_token + every other field; only the Copilot token
        // and its expiry rotate.
        let mut out = secret.clone();
        let obj = out
            .as_object_mut()
            .ok_or_else(|| ChannelError::Build("secret is not an object".into()))?;
        obj.insert("copilot_token".into(), Value::String(resp.token));
        obj.insert(
            "copilot_expires_at_ms".into(),
            Value::Number(expires_at_ms.into()),
        );
        Ok(out)
    }

    fn prepare_usage_request(
        &self,
        secret: &Value,
        settings: &Value,
    ) -> Result<Option<http::Request<Bytes>>, ChannelError> {
        usage::request(secret, settings)
    }

    fn parse_usage(
        &self,
        status: http::StatusCode,
        _headers: &http::HeaderMap,
        body: &Bytes,
    ) -> Option<crate::channel::UsageSnapshot> {
        usage::parse(status, body)
    }
}

/// GitHub device-code login: the operator visits the verification URL with the
/// user code, and the poll mints `{github_token}` (which `refresh` later
/// re-exchanges for the Copilot token). No authcode + no cookie flow.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl ChannelLogin for CopilotCliChannel {
    async fn device_start(
        &self,
        client: &Arc<dyn UpstreamClient>,
        _params: &serde_json::Value,
    ) -> Result<DeviceInit, ChannelError> {
        auth::device_start(client).await
    }

    async fn device_poll(
        &self,
        client: &Arc<dyn UpstreamClient>,
        device_code: &str,
    ) -> Result<DevicePoll, ChannelError> {
        auth::device_poll(client, device_code).await
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use bytes::Bytes;
    use http::{HeaderMap, Method, Response};
    use serde_json::json;

    use crate::http::client::ClientError;

    /// Canned-token mock: every `send` returns the same GitHub token-exchange
    /// JSON, so `refresh` can be exercised without the pipeline.
    struct MockUpstream;
    #[async_trait::async_trait]
    impl UpstreamClient for MockUpstream {
        async fn send(&self, _req: http::Request<Bytes>) -> Result<Response<Bytes>, ClientError> {
            Ok(Response::builder()
                .status(200)
                .body(Bytes::from_static(
                    br#"{"token":"cop-xyz","expires_at":1700000000,"refresh_in":1500}"#,
                ))
                .unwrap())
        }
    }

    #[tokio::test]
    async fn refresh_reexchanges_copilot_token() {
        let secret = json!({ "github_token": "ghu_abc", "account_type": "business" });
        let client: Arc<dyn UpstreamClient> = Arc::new(MockUpstream);
        let out = CopilotCliChannel.refresh(&client, &secret).await.unwrap();

        assert_eq!(out["copilot_token"], "cop-xyz");
        assert_eq!(out["copilot_expires_at_ms"], 1_700_000_000_000i64);
        // github_token + other fields preserved.
        assert_eq!(out["github_token"], "ghu_abc");
        assert_eq!(out["account_type"], "business");
    }

    #[test]
    fn prepare_injects_bearer_and_headers() {
        let secret = json!({
            "github_token": "ghu_abc",
            "copilot_token": "cop-xyz",
            "account_type": "business",
        });
        let settings = json!({});
        let headers = HeaderMap::new();
        let ctx = PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            upstream_model_id: "gpt-4o",
            method: Method::POST,
            path: "/v1/chat/completions",
            query: None,
            headers: &headers,
            body: Bytes::from_static(b"{\"model\":\"gpt-4o\"}"),
        };
        let req = CopilotCliChannel.prepare(ctx).unwrap().request;

        assert_eq!(
            req.uri().to_string(),
            "https://api.business.githubcopilot.com/chat/completions"
        );
        assert_eq!(
            req.headers().get("authorization").unwrap(),
            "Bearer cop-xyz"
        );
        assert_eq!(
            req.headers().get("copilot-integration-id").unwrap(),
            "copilot-developer-cli"
        );
        assert_eq!(
            req.headers().get("editor-version").unwrap(),
            "copilot/1.0.61"
        );
        assert_eq!(
            req.headers().get("openai-intent").unwrap(),
            "conversation-agent"
        );
        assert!(req.headers().get("x-interaction-id").is_some());
        assert!(req.headers().get("x-client-machine-id").is_some());
        // No assistant/tool turn → X-Initiator user.
        assert_eq!(req.headers().get("x-initiator").unwrap(), "user");
    }

    #[test]
    fn prepare_routes_model_list_to_models_endpoint() {
        // A model-list pull is a GET to a `/models` path — it must hit
        // `{base}/models`, NOT `/chat/completions` (which 405s on GET).
        let secret = json!({ "github_token": "ghu_abc", "copilot_token": "cop-xyz" });
        let settings = json!({});
        let headers = HeaderMap::new();
        let ctx = PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            upstream_model_id: "",
            method: Method::GET,
            path: "/v1/models",
            query: None,
            headers: &headers,
            body: Bytes::new(),
        };
        let req = CopilotCliChannel.prepare(ctx).unwrap().request;
        assert_eq!(req.method(), Method::GET);
        assert_eq!(
            req.uri().to_string(),
            "https://api.githubcopilot.com/models"
        );
        assert_eq!(
            req.headers().get("authorization").unwrap(),
            "Bearer cop-xyz"
        );
    }
}
