//! Antigravity channel (Google Code Assist via gcli2api — the same Code Assist
//! envelope as `geminicli`, with a distinct OAuth client and User-Agent).
//!
//! the request is a Gemini
//! `generateContent` body, nested by Code Assist under `request` alongside
//! routing metadata (`{model, project, user_prompt_id, request:<body>}`) with
//! the response under `.response`. [`prepare`](AntigravityChannel::prepare)
//! wraps via [`envelope::wrap_code_assist`]; [`shape_response`] and
//! [`stream_decoder`] unwrap the non-stream body and each SSE frame — all shared
//! with `geminicli`. [`auth`] owns the OAuth bearer + refresh + Antigravity's
//! distinct fingerprint (the `antigravity/cli` User-Agent).
//!
//! [`shape_response`]: AntigravityChannel::shape_response
//! [`stream_decoder`]: AntigravityChannel::stream_decoder

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
use crate::protocol::Provider;

pub struct AntigravityChannel;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for AntigravityChannel {
    fn id(&self) -> &'static str {
        "antigravity"
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

        // Model-pull ListModels: Antigravity has no `/v1beta/models` endpoint —
        // POST an empty body to the bespoke Code Assist `fetchAvailableModels`
        // (same auth + fingerprint as content), reshaped back in shape_response.
        if model_list::is_list_models_request(&ctx.method, ctx.path) {
            let uri = join_url(
                auth::BASE_URL,
                model_list::FETCH_AVAILABLE_MODELS_PATH,
                None,
            )?;
            let headers = allow_headers(ctx.headers, &[]);
            let mut req =
                build_request(http::Method::POST, uri, headers, Bytes::from_static(b"{}"))?;
            auth::apply(&mut req, &access_token)?;
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

        // Verb encoded in the inbound path (shared with geminicli/vertex): the
        // Code Assist endpoints live under `/v1internal:` and stream only when
        // `alt=sse` is set explicitly.
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
        auth::apply(&mut req, &access_token)?;
        Ok(PreparedRequest::new(req))
    }

    fn shape_request(&self, body: Bytes, _headers: &mut http::HeaderMap, _ctx: &ShapeCtx) -> Bytes {
        // Strip generationConfig fields Code Assist rejects, BEFORE the prepare
        // envelope wrap sees the gemini body. Best-effort (no-op on non-JSON).
        shaping::with_json_body(body, gemini_genconfig::strip)
    }

    fn shape_response(&self, body: Bytes, ctx: &ShapeCtx) -> Bytes {
        // ListModels: reshape the bespoke `fetchAvailableModels` payload into the
        // canonical Gemini `{models:[{name:"models/<id>", ...}]}` shape that
        // parse_models reads (the content unwrap/normalize does not apply).
        if ctx.op.operation == crate::protocol::Operation::ListModels {
            return model_list::available_models_to_list_response(body);
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
        envelope::user_quota_request(base, access_token, project_id, auth::USER_AGENT_VALUE)
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
impl ChannelLogin for AntigravityChannel {
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
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use http::{HeaderMap, Method};
    use serde_json::json;

    #[test]
    fn prepare_wraps_envelope_antigravity_ua() {
        let secret = json!({ "access_token": "tok-abc", "project_id": "proj" });
        let settings = json!({});
        let headers = HeaderMap::new();

        let ctx = PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            upstream_model_id: "gemini-2.5-pro",
            method: Method::POST,
            path: "/v1beta/models/gemini-2.5-pro:generateContent",
            query: None,
            headers: &headers,
            body: Bytes::from_static(br#"{"contents":[{"role":"user","parts":[{"text":"hi"}]}]}"#),
        };
        let req = AntigravityChannel.prepare(ctx).unwrap().request;

        // Distinct from geminicli: code-assist path + Antigravity UA/client wiring.
        assert_eq!(
            req.uri().to_string(),
            "https://cloudcode-pa.googleapis.com/v1internal:generateContent"
        );
        assert_eq!(
            req.headers().get("authorization").unwrap(),
            "Bearer tok-abc"
        );
        assert_eq!(
            req.headers().get("user-agent").unwrap(),
            "antigravity/cli/1.0.6 linux/amd64"
        );
        // The real 1.0.6 inference call sends no requestId/requestType/Accept.
        assert!(req.headers().get("requesttype").is_none());
        assert!(req.headers().get("requestid").is_none());
        assert!(req.headers().get("accept").is_none());

        // Body is the Code Assist envelope carrying the original request.
        let v: Value = serde_json::from_slice(req.body()).unwrap();
        assert_eq!(v["model"], "gemini-2.5-pro");
        assert_eq!(v["project"], "proj");
        assert!(v["user_prompt_id"].as_str().is_some_and(|s| s.len() == 32));
        assert_eq!(v["request"]["contents"][0]["parts"][0]["text"], "hi");
    }

    #[test]
    fn prepare_list_models_posts_fetch_available_models() {
        let secret = json!({ "access_token": "tok-abc", "project_id": "proj" });
        let settings = json!({});
        let headers = HeaderMap::new();

        // The admin model-pull sends GET /v1beta/models (no model id, no verb).
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
        let req = AntigravityChannel.prepare(ctx).unwrap().request;

        // Redirected to the bespoke Code Assist fetchAvailableModels POST.
        assert_eq!(req.method(), Method::POST);
        assert_eq!(
            req.uri().to_string(),
            "https://cloudcode-pa.googleapis.com/v1internal:fetchAvailableModels"
        );
        assert_eq!(
            req.headers().get("authorization").unwrap(),
            "Bearer tok-abc"
        );
        assert_eq!(
            req.headers().get("user-agent").unwrap(),
            "antigravity/cli/1.0.6 linux/amd64"
        );
        // Empty JSON object body (v1 fidelity), NOT the Code Assist envelope.
        assert_eq!(req.body().as_ref(), b"{}");
    }

    #[test]
    fn shape_request_strips_genconfig() {
        let shape = ShapeCtx {
            op: crate::protocol::OperationKey::content_generation(
                crate::protocol::Operation::GenerateContent,
                crate::protocol::ContentGenerationKind::GeminiGenerateContent,
            ),
            stream: false,
            status: http::StatusCode::OK,
            enable_magic_cache: false,
        };
        let mut headers = HeaderMap::new();
        let body = Bytes::from(
            json!({
                "contents": [],
                "generationConfig": {"maxOutputTokens": 1024, "responseLogprobs": true, "temperature": 0.5}
            })
            .to_string(),
        );
        let out = AntigravityChannel.shape_request(body, &mut headers, &shape);
        let v: Value = serde_json::from_slice(&out).unwrap();
        let cfg = v["generationConfig"].as_object().unwrap();
        assert!(!cfg.contains_key("maxOutputTokens"));
        assert!(!cfg.contains_key("responseLogprobs"));
        assert_eq!(cfg["temperature"], 0.5);
    }

    #[test]
    fn shape_response_unwraps_then_normalizes() {
        let shape = ShapeCtx {
            op: crate::protocol::OperationKey::content_generation(
                crate::protocol::Operation::GenerateContent,
                crate::protocol::ContentGenerationKind::GeminiGenerateContent,
            ),
            stream: false,
            status: http::StatusCode::OK,
            enable_magic_cache: false,
        };
        // Code Assist envelope wrapping a Vertex-shaped citation block.
        let body = Bytes::from(
            json!({"response": {
                "candidates": [{"citationMetadata": {"citations": [{"uri": "x"}]}}]
            }})
            .to_string(),
        );
        let out = AntigravityChannel.shape_response(body, &shape);
        let v: Value = serde_json::from_slice(&out).unwrap();
        // Envelope unwrapped (no `.response`) AND vertex-normalized.
        assert!(v.get("response").is_none());
        assert_eq!(
            v["candidates"][0]["citationMetadata"]["citationSources"][0]["uri"],
            "x"
        );
    }
}
