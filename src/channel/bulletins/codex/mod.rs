//! Codex channel — OpenAI ChatGPT-backend Responses API over OAuth2
//! (`refresh_token` grant) plus the `codex_exec` impersonation header set.
//!
//! the upstream natively speaks the OpenAI Responses format
//! SSE, so there is NO envelope, NO stream decoder, NO normalize. This channel
//! does, however, SHAPE the request body in [`prepare`](CodexChannel::prepare)
//! (documented body mutation) — forcing `stream`/`store`, stripping sampling
//! fields, and lifting system messages into `instructions` — via
//! [`auth::normalize_responses_body`]. [`auth`] owns the OAuth refresh + the
//! fingerprint headers. The inbound `/v1/responses` path is rewritten to the
//! backend `/responses`.

mod auth;
#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
mod fingerprint;
mod shape;
mod usage;

use std::sync::Arc;

use bytes::Bytes;
use serde_json::Value;

use crate::channel::http_util::{allow_headers, build_request, join_url};
use crate::channel::shaping::{self, openai_cache};
use crate::channel::{
    AuthCodeStart, Channel, ChannelError, ChannelLogin, DeviceInit, DevicePoll, PrepareCtx,
    PreparedRequest, ShapeCtx,
};
use crate::http::client::UpstreamClient;
use crate::protocol::{Operation, Provider};

pub struct CodexChannel;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for CodexChannel {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn provider_family(&self) -> Provider {
        Provider::OpenAi
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        use crate::channel::routes::{cg, local, pass, pv, unsupported, xform};
        use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};
        vec![
            pass(ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Claude), ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Gemini), ListModels, pv(P::OpenAi)),
            pass(GetModel, pv(P::OpenAi)),
            xform(GetModel, pv(P::Claude), GetModel, pv(P::OpenAi)),
            xform(GetModel, pv(P::Gemini), GetModel, pv(P::OpenAi)),
            local(CountTokens, pv(P::OpenAi)),
            local(CountTokens, pv(P::Claude)),
            local(CountTokens, pv(P::Gemini)),
            xform(
                GenerateContent,
                cg(OpenAiResponses),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            xform(
                GenerateContent,
                cg(OpenAiResponsesWebSocket),
                StreamGenerateContent,
                cg(OpenAiResponsesWebSocket),
            ),
            xform(
                GenerateContent,
                cg(OpenAiChatCompletions),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            xform(
                GenerateContent,
                cg(ClaudeMessages),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            xform(
                GenerateContent,
                cg(GeminiGenerateContent),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            pass(StreamGenerateContent, cg(OpenAiResponses)),
            pass(StreamGenerateContent, cg(OpenAiResponsesWebSocket)),
            xform(
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            xform(
                StreamGenerateContent,
                cg(ClaudeMessages),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            xform(
                StreamGenerateContent,
                cg(GeminiGenerateContent),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            xform(
                CreateImage,
                pv(P::OpenAi),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            xform(
                EditImage,
                pv(P::OpenAi),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            unsupported(CreateEmbedding, pv(P::OpenAi)),
            unsupported(CreateEmbedding, pv(P::Gemini)),
            pass(CompactContent, pv(P::OpenAi)),
        ]
    }

    #[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
    fn default_emulation(&self) -> Option<wreq::Emulation> {
        Some(fingerprint::default_emulation())
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let access_token = auth::access_token(ctx.secret)?.to_string();
        let account_id = auth::account_id(ctx.secret).map(str::to_owned);
        let base = ctx
            .provider_settings
            .get("base_url")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(auth::DEFAULT_BASE_URL);

        // The inbound OpenAiResponses path is provider-relative `/v1/responses`
        // (`/v1/responses/compact` for the compact op); the codex backend drops
        // the `/v1` segment — base already ends in `/backend-api/codex`.
        let websocket = crate::channel::responses_websocket::is_target(&ctx.method, ctx.path);
        let path = ctx.path.strip_prefix("/v1").unwrap_or(ctx.path);
        // The model-list / model-get endpoint (`/models[/{id}]`) expects a
        // `client_version` query (v1 parity); content ops keep their own query.
        let models_query =
            (path == "/models" || path.starts_with("/models/")).then(|| match ctx.query {
                Some(q) if !q.is_empty() => format!("{q}&client_version={}", auth::CODEX_VERSION),
                _ => format!("client_version={}", auth::CODEX_VERSION),
            });
        let uri = join_url(base, path, models_query.as_deref().or(ctx.query))?;

        // Shape the Responses body for the ChatGPT backend (force stream/store,
        // strip sampling fields, lift system messages → instructions).
        let body = auth::normalize_responses_body(&ctx.body);

        // Impersonation channel: it injects its own auth + fingerprint headers
        // and forwards the codex protocol headers a client may set (base
        // allow-list adds content-type / accept).
        let headers = allow_headers(
            ctx.headers,
            &[
                "x-codex-beta-features",
                "x-codex-turn-metadata",
                "x-codex-window-id",
                "thread-id",
                "session-id",
                "x-client-request-id",
            ],
        );
        let mut req = build_request(ctx.method, uri, headers, body)?;
        auth::apply(&mut req, &access_token, account_id.as_deref())?;
        if websocket {
            crate::channel::responses_websocket::apply_beta(req.headers_mut());
            *req.uri_mut() = crate::channel::responses_websocket::websocket_uri(req.uri())?;
            return crate::channel::responses_websocket::prepare(req);
        }
        Ok(PreparedRequest::new(req))
    }

    fn shape_request(&self, body: Bytes, _headers: &mut http::HeaderMap, ctx: &ShapeCtx) -> Bytes {
        let Some(kind) = ctx
            .enable_magic_cache
            .then(|| openai_cache::kind_for_operation(ctx.op))
            .flatten()
        else {
            return body;
        };
        shaping::with_json_body(body, |value| {
            openai_cache::apply_magic_string_cache_breakpoints(value, kind)
        })
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

    fn prepare_rate_limit_reset_credit_request(
        &self,
        secret: &Value,
        settings: &Value,
        idempotency_key: &str,
    ) -> Result<Option<http::Request<Bytes>>, ChannelError> {
        usage::reset_credit_request(secret, settings, idempotency_key)
    }

    fn parse_rate_limit_reset_credit(
        &self,
        status: http::StatusCode,
        _headers: &http::HeaderMap,
        body: &Bytes,
    ) -> Option<crate::channel::RateLimitResetCreditConsumeResponse> {
        usage::parse_reset_credit(status, body)
    }

    /// Reshape the codex model catalogue into the OpenAI family canonical shape.
    /// Content ops (Responses passthrough) are returned unchanged — the codex
    /// backend already speaks OpenAI Responses, so there is nothing to reproject.
    fn shape_response(&self, body: Bytes, ctx: &ShapeCtx) -> Bytes {
        match ctx.op.operation {
            Operation::ListModels => shape::shape_model_list(body),
            Operation::GetModel => shape::shape_model_get(body),
            _ => body,
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl ChannelLogin for CodexChannel {
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
            extra: None,
        }))
    }

    async fn authcode_exchange(
        &self,
        client: &Arc<dyn UpstreamClient>,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
        _extra: Option<&Value>,
    ) -> Result<Value, ChannelError> {
        auth::authcode_exchange(client, code, verifier, redirect_uri).await
    }

    async fn device_start(
        &self,
        client: &Arc<dyn UpstreamClient>,
        _params: &Value,
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
    use http::{HeaderMap, Method, StatusCode};
    use serde_json::json;

    use crate::protocol::{ContentGenerationKind as Kind, Operation, OperationKind, Provider};
    use crate::transform::routing::RoutingDecision;

    /// Social `authcode_start` ignores the client; this never sends.
    struct NoopUpstream;
    #[async_trait::async_trait]
    impl UpstreamClient for NoopUpstream {
        async fn send(
            &self,
            _req: http::Request<Bytes>,
        ) -> Result<http::Response<Bytes>, crate::http::client::ClientError> {
            Err(crate::http::client::ClientError::Transport("noop".into()))
        }
    }

    fn prepared_body(body: &'static [u8]) -> Value {
        let secret = json!({ "access_token": "tok-abc" });
        let settings = json!({});
        let headers = HeaderMap::new();
        let ctx = PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            upstream_model_id: "gpt-5.4",
            method: Method::POST,
            path: "/v1/responses",
            query: None,
            headers: &headers,
            body: Bytes::from_static(body),
        };
        let req = CodexChannel.prepare(ctx).unwrap().into_http();
        serde_json::from_slice(req.body()).unwrap()
    }

    fn route(operation: Operation, kind: Kind) -> RoutingDecision {
        CodexChannel
            .routing_table()
            .into_iter()
            .find(|(source, _)| {
                source.operation == operation && source.kind == crate::channel::routes::cg(kind)
            })
            .map(|(_, decision)| decision)
            .expect("missing route")
    }

    fn provider_route(operation: Operation, provider: Provider) -> RoutingDecision {
        CodexChannel
            .routing_table()
            .into_iter()
            .find(|(source, _)| {
                source.operation == operation && source.kind == crate::channel::routes::pv(provider)
            })
            .map(|(_, decision)| decision)
            .expect("missing route")
    }

    #[test]
    fn magic_cache_breakpoint_survives_codex_normalization() {
        let mut headers = HeaderMap::new();
        let shape = ShapeCtx {
            op: crate::protocol::OperationKey::content_generation(
                Operation::StreamGenerateContent,
                Kind::OpenAiResponses,
            ),
            stream: true,
            status: StatusCode::OK,
            enable_magic_cache: true,
            enable_claude_fable_fallback: false,
        };
        let shaped = CodexChannel.shape_request(
            Bytes::from_static(
                br#"{"model":"gpt-5.6","instructions":"stable GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH","input":"hello"}"#,
            ),
            &mut headers,
            &shape,
        );
        let secret = json!({ "access_token": "tok-abc" });
        let settings = json!({});
        let prepared = CodexChannel
            .prepare(PrepareCtx {
                secret: &secret,
                provider_settings: &settings,
                upstream_model_id: "gpt-5.6",
                method: Method::POST,
                path: "/v1/responses",
                query: None,
                headers: &headers,
                body: shaped,
            })
            .unwrap()
            .into_http();
        let value: Value = serde_json::from_slice(prepared.body()).unwrap();
        assert_eq!(
            value["input"][0]["content"][0]["prompt_cache_breakpoint"]["mode"],
            "explicit"
        );
        assert_eq!(value["input"][0]["role"], "developer");
    }

    #[test]
    fn content_defaults_target_streaming_responses_except_websocket_source() {
        for (operation, kind) in [
            (Operation::GenerateContent, Kind::OpenAiResponses),
            (Operation::GenerateContent, Kind::OpenAiChatCompletions),
            (Operation::GenerateContent, Kind::ClaudeMessages),
            (Operation::GenerateContent, Kind::GeminiGenerateContent),
            (
                Operation::StreamGenerateContent,
                Kind::OpenAiChatCompletions,
            ),
            (Operation::StreamGenerateContent, Kind::ClaudeMessages),
            (
                Operation::StreamGenerateContent,
                Kind::GeminiGenerateContent,
            ),
        ] {
            let RoutingDecision::TransformTo(target) = route(operation, kind) else {
                panic!("route should transform to streaming responses");
            };
            assert_eq!(target.operation, Operation::StreamGenerateContent);
            assert_eq!(
                target.kind,
                OperationKind::ContentGeneration(Kind::OpenAiResponses)
            );
        }

        assert_eq!(
            route(Operation::StreamGenerateContent, Kind::OpenAiResponses),
            RoutingDecision::Passthrough
        );

        assert_eq!(
            route(
                Operation::StreamGenerateContent,
                Kind::OpenAiResponsesWebSocket
            ),
            RoutingDecision::Passthrough
        );

        let RoutingDecision::TransformTo(target) =
            route(Operation::GenerateContent, Kind::OpenAiResponsesWebSocket)
        else {
            panic!("websocket source should transform to streaming websocket");
        };
        assert_eq!(target.operation, Operation::StreamGenerateContent);
        assert_eq!(
            target.kind,
            OperationKind::ContentGeneration(Kind::OpenAiResponsesWebSocket)
        );
    }

    #[test]
    fn embeddings_default_to_unsupported() {
        assert_eq!(
            provider_route(Operation::CreateEmbedding, Provider::OpenAi),
            RoutingDecision::Unsupported
        );
        assert_eq!(
            provider_route(Operation::CreateEmbedding, Provider::Gemini),
            RoutingDecision::Unsupported
        );
    }

    #[test]
    fn prepare_responses_websocket_returns_custom_stream() {
        let secret = json!({ "access_token": "tok-abc" });
        let settings = json!({});
        let headers = HeaderMap::new();
        let ctx = PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            upstream_model_id: "gpt-5.4",
            method: Method::GET,
            path: "/v1/responses",
            query: None,
            headers: &headers,
            body: Bytes::from_static(
                br#"{"type":"response.create","model":"gpt-5.4","input":"hi","stream":true}"#,
            ),
        };

        assert!(matches!(
            CodexChannel.prepare(ctx).unwrap(),
            PreparedRequest::CustomStream(_)
        ));
    }

    #[test]
    fn normalizes_responses_body() {
        // String input → forced stream/store, sampling fields dropped, input
        // wrapped as a single user message.
        let v = prepared_body(
            br#"{"model":"gpt-5.4","input":"hi","temperature":0.7,"max_output_tokens":100,"stream":false}"#,
        );
        assert_eq!(v["stream"], json!(true));
        assert_eq!(v["store"], json!(false));
        assert!(v.get("temperature").is_none());
        assert!(v.get("max_output_tokens").is_none());
        assert_eq!(
            v["input"],
            json!([{ "type": "message", "role": "user", "content": "hi" }])
        );

        // System message lifted into instructions; only the user message kept.
        let v = prepared_body(
            br#"{"model":"gpt-5.4","input":[{"role":"system","content":"S"},{"role":"user","content":"U"}]}"#,
        );
        assert_eq!(v["instructions"], json!("S"));
        let roles: Vec<&str> = v["input"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["role"].as_str().unwrap())
            .collect();
        assert_eq!(roles, vec!["user"]);
    }

    #[test]
    fn prepare_url_and_headers() {
        let secret = json!({ "access_token": "tok-abc", "account_id": "acct-9" });
        let settings = json!({});
        let headers = HeaderMap::new();
        let ctx = PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            upstream_model_id: "gpt-5.4",
            method: Method::POST,
            path: "/v1/responses",
            query: None,
            headers: &headers,
            body: Bytes::from_static(br#"{"model":"gpt-5.4","input":"hi"}"#),
        };
        let req = CodexChannel.prepare(ctx).unwrap().into_http();

        assert_eq!(
            req.uri().to_string(),
            "https://chatgpt.com/backend-api/codex/responses"
        );
        assert_eq!(
            req.headers().get("authorization").unwrap(),
            "Bearer tok-abc"
        );
        assert_eq!(req.headers().get("originator").unwrap(), "codex_exec");
        assert_eq!(req.headers().get("chatgpt-account-id").unwrap(), "acct-9");
        // session-id and x-client-request-id share the same generated value.
        assert_eq!(
            req.headers().get("session-id").unwrap(),
            req.headers().get("x-client-request-id").unwrap()
        );
    }

    #[test]
    fn model_list_request_carries_client_version() {
        let secret = json!({ "access_token": "tok-abc" });
        let settings = json!({});
        let headers = HeaderMap::new();
        // The admin model-pull sends a GET `/v1/models` (no query).
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
        let req = CodexChannel.prepare(ctx).unwrap().into_http();
        assert_eq!(
            req.uri().to_string(),
            format!(
                "https://chatgpt.com/backend-api/codex/models?client_version={}",
                auth::CODEX_VERSION
            ),
        );
        // GET with an empty body stays empty (normalize_responses_body no-ops).
        assert!(req.body().is_empty());
    }

    #[test]
    fn forwards_codex_client_headers() {
        let secret = json!({ "access_token": "tok-abc" });
        let settings = json!({});
        let id = "019ebb45-a25d-7520-a8e3-fda4ebc99692";
        let mut headers = HeaderMap::new();
        headers.insert("session-id", id.parse().unwrap());
        headers.insert("thread-id", id.parse().unwrap());
        headers.insert("x-client-request-id", id.parse().unwrap());
        headers.insert("x-codex-window-id", format!("{id}:0").parse().unwrap());
        headers.insert(
            "x-codex-beta-features",
            "terminal_resize_reflow,memories".parse().unwrap(),
        );
        let ctx = PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            upstream_model_id: "gpt-5.4",
            method: Method::POST,
            path: "/v1/responses",
            query: None,
            headers: &headers,
            body: Bytes::from_static(br#"{"input":"hi"}"#),
        };
        let req = CodexChannel.prepare(ctx).unwrap().into_http();
        // A codex-aware client's protocol headers pass through verbatim — GPROXY
        // does NOT regenerate them (so they stay consistent with turn-metadata).
        assert_eq!(req.headers().get("session-id").unwrap(), id);
        assert_eq!(req.headers().get("thread-id").unwrap(), id);
        assert_eq!(req.headers().get("x-client-request-id").unwrap(), id);
        assert_eq!(
            req.headers().get("x-codex-window-id").unwrap(),
            &format!("{id}:0")
        );
        assert_eq!(
            req.headers().get("x-codex-beta-features").unwrap(),
            "terminal_resize_reflow,memories"
        );
        // GPROXY still owns auth/originator/UA.
        assert_eq!(req.headers().get("originator").unwrap(), "codex_exec");
    }

    #[tokio::test]
    async fn codex_authcode_start_url() {
        // Empty redirect_uri → codex default; URL carries the PKCE + state set.
        let client: Arc<dyn UpstreamClient> = Arc::new(NoopUpstream);
        let start = CodexChannel
            .authcode_start(&client, &json!({}), "", "STATE", "CHAL")
            .await
            .expect("authcode_start ok")
            .expect("codex supports authcode");
        let url = &start.authorize_url;
        assert!(url.starts_with("https://auth.openai.com/oauth/authorize?"));
        assert!(
            url.contains("client_id=app_EMoamEEZ73f0CkXaXp7hrann"),
            "{url}"
        );
        assert!(url.contains("code_challenge=CHAL"), "{url}");
        assert!(url.contains("state=STATE"), "{url}");
        assert!(url.contains("code_challenge_method=S256"), "{url}");
        assert!(url.contains("redirect_uri="), "{url}");
        assert_eq!(start.redirect_uri, "http://localhost:1455/auth/callback");
    }
}
