//! Claude Code channel — Anthropic Messages API over OAuth2 (`refresh_token`
//! grant) plus the claude-cli / `@anthropic-ai/sdk` impersonation header set.
//!
//! the request is a verbatim Claude-messages
//! passthrough — NO envelope, NO stream decoder, NO normalize. [`auth`] owns the
//! OAuth refresh + header injection (with a cookie-bootstrap refresh fallback for
//! credentials that carry only a `cookie`).

mod auth;
mod cch;
mod cookie;
#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
mod fingerprint;
mod usage;

use std::sync::Arc;

use bytes::Bytes;
use serde_json::{Value, json};

use crate::channel::http_util::{allow_headers, build_request, join_url};
use crate::channel::shaping::{
    self, claude_cache_control, claude_fallback, claude_magic_cache, claude_sampling,
};
use crate::channel::{
    AuthCodeStart, Channel, ChannelError, ChannelLogin, PrepareCtx, PreparedRequest, ShapeCtx,
};
use crate::http::client::UpstreamClient;
use crate::protocol::{ContentGenerationKind, OperationKind, Provider};

/// Whether `op` targets the Claude-messages content-generation path (the only
/// route that carries a Claude request body to shape).
fn is_claude_messages(op: crate::protocol::OperationKey) -> bool {
    matches!(
        op.kind,
        OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
    )
}

/// Real Claude Code sends model calls to `/v1/messages?beta=true`. Preserve any
/// caller query, but make the fingerprint marker present on the exact model
/// endpoint.
fn model_query(query: Option<&str>) -> String {
    let Some(q) = query.map(str::trim).filter(|q| !q.is_empty()) else {
        return "beta=true".to_owned();
    };
    if q.split('&')
        .any(|pair| pair.split('=').next() == Some("beta"))
    {
        q.to_owned()
    } else {
        format!("beta=true&{q}")
    }
}

pub struct ClaudeCodeChannel;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for ClaudeCodeChannel {
    fn id(&self) -> &'static str {
        "claudecode"
    }

    fn provider_family(&self) -> Provider {
        Provider::Claude
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        use crate::channel::routes::{cg, pass, pv, xform};
        use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};
        vec![
            pass(ListModels, pv(P::Claude)),
            xform(ListModels, pv(P::OpenAi), ListModels, pv(P::Claude)),
            xform(ListModels, pv(P::Gemini), ListModels, pv(P::Claude)),
            pass(GetModel, pv(P::Claude)),
            xform(GetModel, pv(P::OpenAi), GetModel, pv(P::Claude)),
            xform(GetModel, pv(P::Gemini), GetModel, pv(P::Claude)),
            pass(CountTokens, pv(P::Claude)),
            xform(CountTokens, pv(P::OpenAi), CountTokens, pv(P::Claude)),
            xform(CountTokens, pv(P::Gemini), CountTokens, pv(P::Claude)),
            pass(GenerateContent, cg(ClaudeMessages)),
            xform(
                GenerateContent,
                cg(OpenAiChatCompletions),
                GenerateContent,
                cg(ClaudeMessages),
            ),
            xform(
                GenerateContent,
                cg(OpenAiResponses),
                GenerateContent,
                cg(ClaudeMessages),
            ),
            xform(
                GenerateContent,
                cg(GeminiGenerateContent),
                GenerateContent,
                cg(ClaudeMessages),
            ),
            pass(StreamGenerateContent, cg(ClaudeMessages)),
            xform(
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
                StreamGenerateContent,
                cg(ClaudeMessages),
            ),
            xform(
                StreamGenerateContent,
                cg(OpenAiResponses),
                StreamGenerateContent,
                cg(ClaudeMessages),
            ),
            xform(
                StreamGenerateContent,
                cg(GeminiGenerateContent),
                StreamGenerateContent,
                cg(ClaudeMessages),
            ),
            xform(
                CompactContent,
                pv(P::OpenAi),
                GenerateContent,
                cg(ClaudeMessages),
            ),
        ]
    }

    #[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
    fn default_emulation(&self) -> Option<wreq::Emulation> {
        Some(fingerprint::default_emulation())
    }

    /// Claude request 整形: runs BEFORE [`prepare`](Self::prepare), so the
    /// sanitized body is what `cch::apply` later checksums. On the
    /// claude-messages content path, sanitize the body (cache_control hygiene) +
    /// strip sampling params, and drop the `context-1m` beta token. The
    /// `oauth-2025-04-20` token is injected afterward by `auth::apply` and is
    /// unaffected here.
    fn shape_request(&self, body: Bytes, headers: &mut http::HeaderMap, ctx: &ShapeCtx) -> Bytes {
        if !is_claude_messages(ctx.op) {
            return body;
        }
        let body = shaping::with_json_body(body, |v| {
            claude_cache_control::sanitize_claude_body(v);
            claude_sampling::strip_sampling_params(v);
            if ctx.enable_claude_fable_fallback {
                claude_fallback::apply_fable_to_opus48(v, headers);
            }
        });
        shaping::anthropic_beta::strip_beta_tokens(headers, &["context-1m-2025-08-07"]);
        body
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let access_token = auth::access_token(ctx.secret)?.to_string();
        let base = ctx
            .provider_settings
            .get("base_url")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(auth::DEFAULT_BASE_URL);

        // Stable per-credential `device_id`; `session_id` is deterministic per
        // (device, conversation, hour) and capped at ≤1000 slots — the SAME value
        // is sent as `x-claude-code-session-id` and inside `metadata.user_id`.
        let device_id = auth::device_id(ctx.secret);
        let now_secs = crate::util::time::unix_now().max(0) as u64;
        let session_id = cch::session_id(&device_id, &ctx.body, now_secs);

        // The model call (`POST /v1/messages`) carries the CLI billing header +
        // `metadata.user_id`; `cch` is computed over the same canonicalized byte
        // stream as the native Claude Code 2.1.199 rewrite path.
        // Match the path EXACTLY (not by prefix): the sibling
        // `POST /v1/messages/count_tokens` endpoint rejects `metadata`
        // ("metadata: Extra inputs are not permitted"), so it must NOT be
        // treated as a model call. The path is already claude-native here
        // (transform rewrote it), so this also covers openai/gemini callers.
        let is_messages = ctx.method == http::Method::POST && ctx.path == "/v1/messages";
        let body = if is_messages {
            let account_uuid = ctx
                .secret
                .get("account_uuid")
                .and_then(Value::as_str)
                .unwrap_or_default();
            // Opt-in magic-string cache triggers (provider `enable_magic_cache`) before cch.
            Bytes::from(cch::apply(
                &claude_magic_cache::apply_if_enabled(ctx.body, ctx.provider_settings),
                &device_id,
                account_uuid,
                &session_id,
                "sdk-cli",
            ))
        } else {
            ctx.body
        };

        // Claude-messages passthrough: the inbound path is already provider
        // relative (`/v1/messages`, `/v1/messages/count_tokens`, …); forward it.
        let model_query = is_messages.then(|| model_query(ctx.query));
        let uri = join_url(base, ctx.path, model_query.as_deref().or(ctx.query))?;
        // Impersonation channel: it injects its own fingerprint headers and only
        // forwards `anthropic-beta` from the client (base allow-list adds
        // content-type / accept).
        let headers = allow_headers(ctx.headers, &["anthropic-beta"]);
        let mut req = build_request(ctx.method, uri, headers, body)?;
        // `x-client-request-id` rides only the direct api.anthropic.com model call.
        let with_request_id = is_messages && base == auth::DEFAULT_BASE_URL;
        auth::apply(&mut req, &access_token, &session_id, with_request_id)?;
        Ok(PreparedRequest::new(req))
    }

    fn needs_refresh(&self, secret: &Value) -> bool {
        auth::needs_refresh(secret)
    }

    async fn refresh(
        &self,
        client: &Arc<dyn UpstreamClient>,
        secret: &Value,
    ) -> Result<Value, ChannelError> {
        auth::refresh(client, secret).await
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

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl ChannelLogin for ClaudeCodeChannel {
    async fn authcode_start(
        &self,
        _client: &Arc<dyn UpstreamClient>,
        _params: &Value,
        redirect_uri: &str,
        state: &str,
        pkce_challenge: &str,
    ) -> Result<Option<AuthCodeStart>, ChannelError> {
        let (authorize_url, redirect_uri) =
            auth::authcode_start(redirect_uri, state, pkce_challenge);
        Ok(Some(AuthCodeStart {
            authorize_url,
            redirect_uri,
            extra: Some(json!({ "state": state })),
        }))
    }

    async fn authcode_exchange(
        &self,
        client: &Arc<dyn UpstreamClient>,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
        extra: Option<&Value>,
    ) -> Result<Value, ChannelError> {
        let state = extra
            .and_then(|v| v.get("state"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ChannelError::Build("claudecode login session missing state".into()))?;
        auth::authcode_exchange(client, code, verifier, redirect_uri, state).await
    }

    async fn cookie_exchange(
        &self,
        client: &Arc<dyn UpstreamClient>,
        cookie: &str,
    ) -> Result<Value, ChannelError> {
        cookie::exchange(client, cookie).await
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use bytes::Bytes;
    use http::{HeaderMap, Method, Response};
    use serde_json::json;

    use crate::http::client::ClientError;

    /// Canned token-endpoint mock: returns a rotated access/refresh pair so
    /// `refresh` can be exercised without the pipeline.
    struct MockUpstream;
    #[async_trait::async_trait]
    impl UpstreamClient for MockUpstream {
        async fn send(&self, _req: http::Request<Bytes>) -> Result<Response<Bytes>, ClientError> {
            Ok(Response::builder()
                .status(200)
                .body(Bytes::from_static(
                    br#"{"access_token":"new","refresh_token":"newrt","expires_in":3600}"#,
                ))
                .unwrap())
        }
    }

    #[tokio::test]
    async fn refresh_rotates_tokens() {
        let secret = json!({
            "access_token": "old",
            "refresh_token": "oldrt",
            "expires_at_ms": 1,
            "account_uuid": "acct-123",
        });
        let client: Arc<dyn UpstreamClient> = Arc::new(MockUpstream);
        let out = ClaudeCodeChannel.refresh(&client, &secret).await.unwrap();

        assert_eq!(out["access_token"], "new");
        assert_eq!(out["refresh_token"], "newrt"); // rotated, not the old one
        assert!(out["expires_at_ms"].as_i64().unwrap() > 0);
        // Unrelated fields preserved.
        assert_eq!(out["account_uuid"], "acct-123");
    }

    #[test]
    fn prepare_injects_oauth_and_stainless() {
        let secret = json!({ "access_token": "tok-abc" });
        let settings = json!({});
        let headers = HeaderMap::new();
        let ctx = PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            upstream_model_id: "claude-sonnet-4",
            method: Method::POST,
            path: "/v1/messages",
            query: None,
            headers: &headers,
            body: Bytes::from_static(b"{\"model\":\"claude-sonnet-4\"}"),
        };
        let req = ClaudeCodeChannel.prepare(ctx).unwrap().into_http();

        assert_eq!(
            req.uri().to_string(),
            "https://api.anthropic.com/v1/messages?beta=true"
        );
        assert_eq!(
            req.headers().get("authorization").unwrap(),
            "Bearer tok-abc"
        );
        assert_eq!(
            req.headers().get("anthropic-beta").unwrap(),
            "oauth-2025-04-20"
        );
        assert_eq!(req.headers().get("x-app").unwrap(), "cli");
        assert_eq!(req.headers().get("x-stainless-lang").unwrap(), "js");
        assert_eq!(
            req.headers().get("x-stainless-package-version").unwrap(),
            "0.94.0"
        );
        assert_eq!(req.headers().get("x-stainless-runtime").unwrap(), "node");
        assert_eq!(
            req.headers().get("user-agent").unwrap(),
            "claude-cli/2.1.199 (external, sdk-cli)"
        );
        assert_eq!(
            req.headers().get("x-stainless-runtime-version").unwrap(),
            "v26.3.0"
        );
        assert!(req.headers().get("x-claude-code-session-id").is_some());
    }

    #[test]
    fn model_query_adds_beta_true_without_touching_other_endpoints() {
        assert_eq!(model_query(None), "beta=true");
        assert_eq!(model_query(Some("foo=1")), "beta=true&foo=1");
        assert_eq!(model_query(Some("beta=true&foo=1")), "beta=true&foo=1");
    }

    #[test]
    fn anthropic_beta_oauth_first_then_client_deduped() {
        let secret = json!({ "access_token": "tok" });
        let settings = json!({});
        let mut headers = HeaderMap::new();
        headers.insert(
            "anthropic-beta",
            "feat-x,oauth-2025-04-20,feat-y".parse().unwrap(),
        );
        let ctx = PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            upstream_model_id: "claude-sonnet-4",
            method: Method::POST,
            path: "/v1/messages",
            query: None,
            headers: &headers,
            body: Bytes::from_static(b"{\"messages\":[]}"),
        };
        let req = ClaudeCodeChannel.prepare(ctx).unwrap().into_http();
        // oauth marker first, client betas after, client's own oauth not duped.
        assert_eq!(
            req.headers().get("anthropic-beta").unwrap(),
            "oauth-2025-04-20,feat-x,feat-y"
        );
    }

    #[test]
    fn count_tokens_skips_cch_metadata_injection() {
        // `POST /v1/messages/count_tokens` is NOT the model call: Anthropic's
        // count endpoint rejects `metadata` ("Extra inputs are not permitted"),
        // so cch injection must be skipped. Regression: the old prefix match
        // (`starts_with("/v1/messages")`) injected it → 400.
        let secret = json!({ "access_token": "tok", "account_uuid": "acct-1" });
        let settings = json!({});
        let headers = HeaderMap::new();
        let body = Bytes::from_static(b"{\"model\":\"claude-haiku-4-5\",\"messages\":[]}");
        let prepare = |path| {
            let req = ClaudeCodeChannel
                .prepare(PrepareCtx {
                    secret: &secret,
                    provider_settings: &settings,
                    upstream_model_id: "claude-haiku-4-5",
                    method: Method::POST,
                    path,
                    query: None,
                    headers: &headers,
                    body: body.clone(),
                })
                .unwrap()
                .into_http();
            serde_json::from_slice::<Value>(req.body()).unwrap()
        };
        // count_tokens: untouched — no metadata.
        let count = prepare("/v1/messages/count_tokens");
        assert!(count.get("metadata").is_none(), "count body: {count}");
        // the real model call still gets metadata.user_id injected.
        let msg = prepare("/v1/messages");
        assert!(msg["metadata"]["user_id"].is_string(), "msg body: {msg}");
    }

    #[tokio::test]
    async fn authcode_start_urls() {
        // claudecode + geminicli authcode_start build provider authorize URLs
        // carrying their client_id, the PKCE challenge, state, S256, a default
        // redirect_uri, and their scopes. (Social: the client/params are unused.)
        let client: Arc<dyn UpstreamClient> = Arc::new(MockUpstream);
        let cc = ClaudeCodeChannel
            .authcode_start(&client, &json!({}), "", "ST", "CH")
            .await
            .expect("authcode_start ok")
            .expect("claudecode supports authcode");
        let url = &cc.authorize_url;
        assert!(
            url.starts_with("https://claude.ai/oauth/authorize?"),
            "{url}"
        );
        assert!(
            url.contains("client_id=9d1c250a-e61b-44d9-88ed-5944d1962f5e"),
            "{url}"
        );
        assert!(url.contains("code_challenge=CH"), "{url}");
        assert!(url.contains("state=ST"), "{url}");
        assert!(url.contains("code_challenge_method=S256"), "{url}");
        assert!(url.contains("redirect_uri="), "{url}");
        assert!(url.contains("scope=user%3Aprofile"), "{url}");
        assert_eq!(cc.redirect_uri, super::auth::DEFAULT_REDIRECT_URI);
        assert_eq!(
            cc.extra.as_ref().and_then(|v| v["state"].as_str()),
            Some("ST")
        );

        let gc = crate::channel::bulletins::geminicli::GeminiCliChannel
            .authcode_start(&client, &json!({}), "", "ST", "CH")
            .await
            .expect("authcode_start ok")
            .expect("geminicli supports authcode");
        let url = &gc.authorize_url;
        assert!(
            url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"),
            "{url}"
        );
        assert!(url.contains("681255809395-"), "{url}");
        assert!(url.contains("code_challenge=CH"), "{url}");
        assert!(url.contains("state=ST"), "{url}");
        assert!(url.contains("code_challenge_method=S256"), "{url}");
        assert!(url.contains("redirect_uri="), "{url}");
        assert!(url.contains("cloud-platform"), "{url}");
    }

    fn messages_ctx() -> ShapeCtx {
        use crate::protocol::{Operation, OperationKey};
        ShapeCtx {
            op: OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::ClaudeMessages,
            ),
            stream: false,
            status: http::StatusCode::OK,
            enable_magic_cache: false,
            enable_claude_fable_fallback: false,
        }
    }

    fn fallback_ctx() -> ShapeCtx {
        ShapeCtx {
            enable_claude_fable_fallback: true,
            ..messages_ctx()
        }
    }

    #[test]
    fn shape_request_strips_sampling_and_context_1m_keeps_oauth() {
        let mut headers = HeaderMap::new();
        // oauth marker must survive the strip; context-1m must go.
        headers.insert(
            "anthropic-beta",
            "oauth-2025-04-20,context-1m-2025-08-07".parse().unwrap(),
        );
        let body = Bytes::from_static(
            br#"{"model":"claude-opus-4-8","messages":[],"temperature":0.7,"top_p":0.9,"top_k":40}"#,
        );
        let out = ClaudeCodeChannel.shape_request(body, &mut headers, &messages_ctx());

        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        let map = v.as_object().unwrap();
        assert!(!map.contains_key("temperature"));
        assert!(!map.contains_key("top_p"));
        assert!(!map.contains_key("top_k"));
        // oauth survives, context-1m stripped.
        assert_eq!(headers.get("anthropic-beta").unwrap(), "oauth-2025-04-20");
    }

    #[test]
    fn shape_request_non_messages_op_is_identity() {
        use crate::protocol::{Operation, OperationKey};
        let mut headers = HeaderMap::new();
        headers.insert("anthropic-beta", "context-1m-2025-08-07".parse().unwrap());
        let body = Bytes::from_static(b"{\"temperature\":0.7}");
        let ctx = ShapeCtx {
            op: OperationKey::provider(Operation::ListModels, super::Provider::Claude),
            stream: false,
            status: http::StatusCode::OK,
            enable_magic_cache: false,
            enable_claude_fable_fallback: false,
        };
        let out = ClaudeCodeChannel.shape_request(body.clone(), &mut headers, &ctx);
        assert_eq!(out, body);
        assert_eq!(
            headers.get("anthropic-beta").unwrap(),
            "context-1m-2025-08-07"
        );
    }

    #[test]
    fn shape_request_injects_fable_fallback_and_keeps_oauth_beta() {
        let mut headers = HeaderMap::new();
        headers.insert("anthropic-beta", "oauth-2025-04-20".parse().unwrap());
        let body =
            Bytes::from_static(br#"{"model":"claude-fable-5","messages":[],"max_tokens":32}"#);
        let out = ClaudeCodeChannel.shape_request(body, &mut headers, &fallback_ctx());

        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(
            v["fallbacks"],
            serde_json::json!([{ "model": "claude-opus-4-8" }])
        );
        assert_eq!(
            headers.get("anthropic-beta").unwrap(),
            "oauth-2025-04-20,server-side-fallback-2026-06-01"
        );
    }
}
