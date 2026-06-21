//! Gemini CLI channel (Google Code Assist, OAuth2 `refresh_token` grant; the
//! Code Assist envelope). FIRST envelope channel — `antigravity` reuses this
//! shape.
//!
//! the request is a Gemini
//! `generateContent` body, but Code Assist nests it under `request` alongside
//! routing metadata (`{model, project, user_prompt_id, request:<body>}`) and
//! nests the response under `.response`. [`prepare`](GeminiCliChannel::prepare)
//! wraps via [`envelope::wrap_code_assist`]; [`shape_response`] and
//! [`stream_decoder`] unwrap the non-stream body and each SSE frame.
//! [`auth`] owns the OAuth bearer + refresh + the CLI fingerprint headers.
//!
//! [`shape_response`]: GeminiCliChannel::shape_response
//! [`stream_decoder`]: GeminiCliChannel::stream_decoder

mod auth;
#[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
mod fingerprint;
mod model_list;

use std::sync::Arc;

use bytes::Bytes;
use serde_json::Value;

use crate::channel::envelope::{self, CodeAssistStreamDecoder};
use crate::channel::http_util::{allow_headers, build_request, join_url};
use crate::channel::shaping::{self, gemini_genconfig, vertex_normalize};
use crate::channel::{
    AuthCodeStart, Channel, ChannelError, ChannelLogin, ChannelStreamDecoder, PrepareCtx,
    PreparedRequest, ShapeCtx,
};
use crate::http::client::UpstreamClient;
use crate::protocol::{Operation, Provider};

pub struct GeminiCliChannel;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for GeminiCliChannel {
    fn id(&self) -> &'static str {
        "geminicli"
    }

    fn provider_family(&self) -> Provider {
        Provider::Gemini
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        use crate::channel::routes::{cg, pass, pv, xform};
        use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};
        vec![
            pass(ListModels, pv(P::Gemini)),
            xform(ListModels, pv(P::Claude), ListModels, pv(P::Gemini)),
            xform(ListModels, pv(P::OpenAi), ListModels, pv(P::Gemini)),
            pass(GetModel, pv(P::Gemini)),
            xform(GetModel, pv(P::Claude), GetModel, pv(P::Gemini)),
            xform(GetModel, pv(P::OpenAi), GetModel, pv(P::Gemini)),
            pass(CountTokens, pv(P::Gemini)),
            xform(CountTokens, pv(P::Claude), CountTokens, pv(P::Gemini)),
            xform(CountTokens, pv(P::OpenAi), CountTokens, pv(P::Gemini)),
            pass(GenerateContent, cg(GeminiGenerateContent)),
            xform(
                GenerateContent,
                cg(ClaudeMessages),
                GenerateContent,
                cg(GeminiGenerateContent),
            ),
            xform(
                GenerateContent,
                cg(OpenAiChatCompletions),
                GenerateContent,
                cg(GeminiGenerateContent),
            ),
            xform(
                GenerateContent,
                cg(OpenAiResponses),
                GenerateContent,
                cg(GeminiGenerateContent),
            ),
            pass(StreamGenerateContent, cg(GeminiGenerateContent)),
            xform(
                StreamGenerateContent,
                cg(ClaudeMessages),
                StreamGenerateContent,
                cg(GeminiGenerateContent),
            ),
            xform(
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
                StreamGenerateContent,
                cg(GeminiGenerateContent),
            ),
            xform(
                StreamGenerateContent,
                cg(OpenAiResponses),
                StreamGenerateContent,
                cg(GeminiGenerateContent),
            ),
            xform(
                CreateImage,
                pv(P::OpenAi),
                GenerateContent,
                cg(GeminiGenerateContent),
            ),
            xform(
                EditImage,
                pv(P::OpenAi),
                GenerateContent,
                cg(GeminiGenerateContent),
            ),
            pass(CreateEmbedding, pv(P::Gemini)),
            xform(
                CreateEmbedding,
                pv(P::OpenAi),
                CreateEmbedding,
                pv(P::Gemini),
            ),
            xform(
                CompactContent,
                pv(P::OpenAi),
                GenerateContent,
                cg(GeminiGenerateContent),
            ),
        ]
    }

    #[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
    fn default_emulation(&self) -> Option<wreq::Emulation> {
        Some(fingerprint::default_emulation())
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let access_token = auth::access_token(ctx.secret)?.to_string();
        let project_id = auth::project_id(ctx.secret)?;

        // Code Assist has no model-list endpoint; the model-pull (GET
        // `/v1beta/models`) is served by deriving the usable models from the
        // per-credential quota. Issue `POST /v1internal:retrieveUserQuota`
        // (reusing the usage plumbing) instead of the content envelope. The
        // bespoke response is reshaped to canonical Gemini in `shape_response`.
        if model_list::is_list_models(&ctx.method, ctx.path) {
            let ua = auth::user_agent(ctx.upstream_model_id);
            let req = envelope::user_quota_request(auth::BASE_URL, &access_token, project_id, &ua)?
                .ok_or_else(|| ChannelError::Build("failed to build retrieveUserQuota".into()))?;
            return Ok(PreparedRequest::new(req));
        }

        // Wrap the gemini body in the Code Assist envelope. `ctx.body` is moved
        // out here (the request body becomes the wrapped bytes). `:countTokens`
        // takes a different envelope (no model/project; request is a plain
        // GenerateContentRequest) — see `wrap_code_assist_count`.
        let is_count = ctx.path.contains(":countTokens");
        let wrapped = if is_count {
            envelope::wrap_code_assist_count(&ctx.body)?
        } else {
            envelope::wrap_code_assist(
                &ctx.body,
                ctx.upstream_model_id,
                project_id,
                &envelope::random_user_prompt_id(),
            )?
        };

        // M2 encodes the verb in the path for gemini targets; reuse only the
        // stream flag (`:streamGenerateContent` → SSE, else `:generateContent`),
        // same as vertex. The Code Assist endpoints live under `/v1internal:`
        // and stream only when `alt=sse` is set explicitly.
        let (verb, query) = if ctx.path.contains(":streamGenerateContent") {
            (":streamGenerateContent", Some("alt=sse"))
        } else if is_count {
            (":countTokens", None)
        } else {
            (":generateContent", None)
        };
        let path = format!("/v1internal{verb}");

        let uri = join_url(auth::BASE_URL, &path, query)?;
        // Envelope channel: it injects its own auth + fingerprint; forward no
        // inbound headers beyond the base content-type/accept allow-list.
        let headers = allow_headers(ctx.headers, &[]);
        let mut req = build_request(ctx.method, uri, headers, Bytes::from(wrapped))?;
        auth::apply(&mut req, &access_token, ctx.upstream_model_id)?;
        Ok(PreparedRequest::new(req))
    }

    fn shape_request(&self, body: Bytes, _headers: &mut http::HeaderMap, _ctx: &ShapeCtx) -> Bytes {
        // Strip generationConfig fields Code Assist rejects, BEFORE the prepare
        // envelope wrap sees the gemini body. Best-effort (no-op on non-JSON).
        shaping::with_json_body(body, gemini_genconfig::strip)
    }

    fn shape_response(&self, body: Bytes, ctx: &ShapeCtx) -> Bytes {
        // ListModels is served by the bespoke `retrieveUserQuota` endpoint;
        // reshape its `buckets` into the canonical Gemini `{"models":[…]}` shape
        // that `parse_models` reads.
        if ctx.op.operation == Operation::ListModels {
            return model_list::quota_to_model_list(body);
        }
        // Unwrap the Code Assist `.response` envelope, then normalize the
        // Vertex/Code-Assist shape to AI-Studio (citation rename, block reason).
        let unwrapped = envelope::unwrap_code_assist(body);
        vertex_normalize::normalize_vertex_response(unwrapped)
    }

    fn stream_decoder(&self) -> Option<Box<dyn ChannelStreamDecoder>> {
        Some(Box::new(CodeAssistStreamDecoder::new()))
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
        let access_token = auth::access_token(secret)?;
        let project_id = auth::project_id(secret)?;
        let base = settings
            .get("base_url")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(auth::BASE_URL);
        // The usage call carries no model; the UA embeds a representative one.
        let ua = auth::user_agent("gemini-2.5-pro");
        envelope::user_quota_request(base, access_token, project_id, &ua)
    }

    fn parse_usage(
        &self,
        status: http::StatusCode,
        _headers: &http::HeaderMap,
        body: &Bytes,
    ) -> Option<crate::channel::UsageSnapshot> {
        envelope::parse_user_quota(status, body)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl ChannelLogin for GeminiCliChannel {
    async fn authcode_start(
        &self,
        _client: &Arc<dyn UpstreamClient>,
        params: &Value,
        redirect_uri: &str,
        state: &str,
        pkce_challenge: &str,
    ) -> Result<Option<AuthCodeStart>, ChannelError> {
        // Two login modes the operator picks via `params.code_only`:
        //   * code-only (default / true) — Google renders the code on
        //     `codeassist.google.com/authcode`; no listener, operator pastes it.
        //   * callback-URL (false) — the loopback `127.0.0.1:1455/oauth2callback`
        //     the console/CLI catches. An explicit `redirect_uri` overrides both.
        let effective = if !redirect_uri.trim().is_empty() {
            redirect_uri
        } else if params.get("code_only").and_then(Value::as_bool) == Some(false) {
            auth::LOOPBACK_REDIRECT_URI
        } else {
            auth::DEFAULT_REDIRECT_URI
        };
        let (authorize_url, redirect_uri) = auth::authcode_start(effective, state, pkce_challenge);
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
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use http::{HeaderMap, Method};
    use serde_json::json;

    fn ctx_for<'a>(
        secret: &'a Value,
        settings: &'a Value,
        headers: &'a HeaderMap,
        path: &'a str,
        body: &'static [u8],
    ) -> PrepareCtx<'a> {
        PrepareCtx {
            secret,
            provider_settings: settings,
            upstream_model_id: "gemini-2.5-pro",
            method: Method::POST,
            path,
            query: None,
            headers,
            body: Bytes::from_static(body),
        }
    }

    #[test]
    fn prepare_wraps_envelope_and_builds_v1internal() {
        let secret = json!({ "access_token": "tok-abc", "project_id": "proj" });
        let settings = json!({});
        let headers = HeaderMap::new();

        // Non-stream: :generateContent, no query, body is the envelope.
        let ctx = ctx_for(
            &secret,
            &settings,
            &headers,
            "/v1beta/models/gemini-2.5-pro:generateContent",
            br#"{"contents":[{"role":"user","parts":[{"text":"hi"}]}]}"#,
        );
        let req = GeminiCliChannel.prepare(ctx).unwrap().request;
        assert_eq!(
            req.uri().to_string(),
            "https://cloudcode-pa.googleapis.com/v1internal:generateContent"
        );
        assert_eq!(
            req.headers().get("authorization").unwrap(),
            "Bearer tok-abc"
        );
        // User-agent embeds the requested model id (dynamic per request).
        assert_eq!(
            req.headers().get("user-agent").unwrap(),
            "GeminiCLI-tui/0.46.0/gemini-2.5-pro (linux; x64; terminal) google-api-nodejs-client/9.15.1"
        );

        // Body is the Code Assist envelope carrying the original request.
        let v: Value = serde_json::from_slice(req.body()).unwrap();
        assert_eq!(v["model"], "gemini-2.5-pro");
        assert_eq!(v["project"], "proj");
        assert!(v["user_prompt_id"].as_str().is_some_and(|s| s.len() == 32));
        assert_eq!(v["request"]["contents"][0]["parts"][0]["text"], "hi");

        // Stream: :streamGenerateContent?alt=sse.
        let ctx = ctx_for(
            &secret,
            &settings,
            &headers,
            "/v1beta/models/gemini-2.5-pro:streamGenerateContent",
            br#"{"contents":[]}"#,
        );
        let req = GeminiCliChannel.prepare(ctx).unwrap().request;
        assert_eq!(
            req.uri().to_string(),
            "https://cloudcode-pa.googleapis.com/v1internal:streamGenerateContent?alt=sse"
        );
    }

    #[test]
    fn list_models_builds_retrieve_user_quota() {
        // The model-pull sends GET /v1beta/models; geminicli has no model-list
        // endpoint, so it must POST /v1internal:retrieveUserQuota with
        // `{"project":<id>}` instead of the content envelope.
        let secret = json!({ "access_token": "tok-abc", "project_id": "proj" });
        let settings = json!({});
        let headers = HeaderMap::new();
        let ctx = PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            upstream_model_id: "",
            method: Method::GET,
            path: "/v1beta/models",
            query: None,
            headers: &headers,
            body: Bytes::new(),
        };
        let req = GeminiCliChannel.prepare(ctx).unwrap().request;
        assert_eq!(req.method(), Method::POST);
        assert_eq!(
            req.uri().to_string(),
            "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota"
        );
        assert_eq!(
            req.headers().get("authorization").unwrap(),
            "Bearer tok-abc"
        );
        let v: Value = serde_json::from_slice(req.body()).unwrap();
        assert_eq!(v, json!({ "project": "proj" }));
    }

    #[test]
    fn shape_response_reshapes_quota_to_model_list() {
        // ListModels op: the bespoke retrieveUserQuota buckets become canonical
        // Gemini `{"models":[…]}`.
        let shape = crate::channel::ShapeCtx {
            op: crate::protocol::OperationKey::provider(
                crate::protocol::Operation::ListModels,
                crate::protocol::Provider::Gemini,
            ),
            stream: false,
            status: http::StatusCode::OK,
            enable_magic_cache: false,
        };
        let out = GeminiCliChannel.shape_response(
            Bytes::from(
                json!({"buckets": [
                    {"modelId": "gemini-2.5-pro", "tokenType": "REQUESTS"}
                ]})
                .to_string(),
            ),
            &shape,
        );
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["models"][0]["name"], "models/gemini-2.5-pro");
    }

    #[test]
    fn normalize_unwraps_and_needs_refresh_expiry() {
        // shape_response extracts `.response`.
        let shape = crate::channel::ShapeCtx {
            op: crate::protocol::OperationKey::content_generation(
                crate::protocol::Operation::GenerateContent,
                crate::protocol::ContentGenerationKind::GeminiGenerateContent,
            ),
            stream: false,
            status: http::StatusCode::OK,
            enable_magic_cache: false,
        };
        let out = GeminiCliChannel.shape_response(
            Bytes::from_static(br#"{"response":{"candidates":[]}}"#),
            &shape,
        );
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v, json!({"candidates": []}));

        // shape_response also vertex-normalizes the unwrapped body: citations →
        // citationSources (still unwraps `.response`).
        let out = GeminiCliChannel.shape_response(
            Bytes::from(
                json!({"response": {
                    "candidates": [{"citationMetadata": {"citations": [{"uri": "x"}]}}]
                }})
                .to_string(),
            ),
            &shape,
        );
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert!(v.get("response").is_none());
        assert_eq!(
            v["candidates"][0]["citationMetadata"]["citationSources"][0]["uri"],
            "x"
        );

        // shape_request strips generationConfig fields Code Assist rejects.
        let mut req_headers = HeaderMap::new();
        let out = GeminiCliChannel.shape_request(
            Bytes::from(
                json!({"generationConfig": {"maxOutputTokens": 8, "temperature": 0.5}}).to_string(),
            ),
            &mut req_headers,
            &shape,
        );
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert!(
            v["generationConfig"]
                .as_object()
                .unwrap()
                .get("maxOutputTokens")
                .is_none()
        );
        assert_eq!(v["generationConfig"]["temperature"], 0.5);

        // needs_refresh: missing project_id errors cleanly in prepare.
        let settings = json!({});
        let headers = HeaderMap::new();
        let no_proj = json!({ "access_token": "t" });
        let err = GeminiCliChannel
            .prepare(ctx_for(
                &no_proj,
                &settings,
                &headers,
                "/x:generateContent",
                b"{}",
            ))
            .unwrap_err();
        assert!(matches!(err, ChannelError::InvalidCredential(m) if m.contains("project_id")));

        // needs_refresh: no token → refresh; near-expiry → refresh; fresh → no.
        let now_ms = crate::util::time::unix_now().saturating_mul(1000);
        assert!(GeminiCliChannel.needs_refresh(&json!({})));
        assert!(GeminiCliChannel.needs_refresh(&json!({
            "access_token": "t", "expires_at_ms": now_ms + 10_000,
        })));
        assert!(!GeminiCliChannel.needs_refresh(&json!({
            "access_token": "t", "expires_at_ms": now_ms + 600_000,
        })));
    }

    /// `authcode_start` never sends; this client must stay unused.
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

    /// `params.code_only` picks the redirect: default / `true` → the headless
    /// `codeassist.google.com/authcode` (paste the code); `false` → the loopback
    /// `127.0.0.1:1455/oauth2callback` the console catches. An explicit
    /// `redirect_uri` hint overrides both. The authorize URL carries the matching
    /// (percent-encoded) redirect each way.
    #[tokio::test]
    async fn authcode_start_selects_redirect_by_code_only() {
        let client: Arc<dyn UpstreamClient> = Arc::new(NoopUpstream);

        // Default (no params) → code-only / codeassist.
        let start = GeminiCliChannel
            .authcode_start(&client, &json!({}), "", "ST", "CH")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(start.redirect_uri, "https://codeassist.google.com/authcode");
        assert!(
            start
                .authorize_url
                .contains("redirect_uri=https%3A%2F%2Fcodeassist.google.com%2Fauthcode")
        );

        // code_only = true → same headless default.
        let start = GeminiCliChannel
            .authcode_start(&client, &json!({ "code_only": true }), "", "ST", "CH")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(start.redirect_uri, "https://codeassist.google.com/authcode");

        // code_only = false → callback-URL loopback.
        let start = GeminiCliChannel
            .authcode_start(&client, &json!({ "code_only": false }), "", "ST", "CH")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(start.redirect_uri, "http://127.0.0.1:1455/oauth2callback");
        assert!(
            start
                .authorize_url
                .contains("127.0.0.1%3A1455%2Foauth2callback")
        );

        // Explicit redirect_uri hint wins over code_only.
        let start = GeminiCliChannel
            .authcode_start(
                &client,
                &json!({ "code_only": true }),
                "http://127.0.0.1:9999/cb",
                "ST",
                "CH",
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(start.redirect_uri, "http://127.0.0.1:9999/cb");
    }
}
