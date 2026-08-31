//! Vertex AI Express channel — api key in the `?key=` query param, default
//! `https://aiplatform.googleapis.com`.

mod auth;

use std::borrow::Cow;

use bytes::Bytes;

use crate::channel::bulletins::common::{self, ApiKeyDefaults};
use crate::channel::http_util::{
    allow_headers_with_settings, allow_query_with_settings, build_request,
};
use crate::channel::shaping::{self, gemini_genconfig, vertex_normalize};
use crate::channel::{Channel, ChannelError, ModelCatalog, PrepareCtx, PreparedRequest, ShapeCtx};
use crate::protocol::{ContentGenerationKind, OperationKind, Provider};

const DEFAULTS: ApiKeyDefaults = ApiKeyDefaults {
    default_base_url: Some("https://aiplatform.googleapis.com"),
    forward_headers: &[],
    // `alt` (sse) + list-models pagination (gemini wire)
    forward_query: &["alt", "pageSize", "pageToken"],
};

/// Whether this op is a Gemini content-generation call (the only response shape
/// VertexExpress normalizes; everything else passes through untouched).
fn is_gemini_content(ctx: &ShapeCtx) -> bool {
    matches!(
        ctx.op.kind(),
        OperationKind::ContentGeneration(ContentGenerationKind::GeminiGenerateContent)
    )
}

/// Express mode v1 expects a fully qualified publisher-model resource. Keep
/// unrelated Gemini paths unchanged so the channel's other routing behavior
/// and exact endpoint overrides retain their existing semantics.
fn default_request_path(path: &str) -> Cow<'_, str> {
    let Some(model_and_verb) = path.strip_prefix("/v1beta/models/") else {
        return Cow::Borrowed(path);
    };
    if [":generateContent", ":streamGenerateContent", ":countTokens"]
        .iter()
        .any(|verb| model_and_verb.ends_with(verb))
    {
        Cow::Owned(format!("/v1/publishers/google/models/{model_and_verb}"))
    } else {
        Cow::Borrowed(path)
    }
}

pub struct VertexExpressChannel;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for VertexExpressChannel {
    fn id(&self) -> &'static str {
        "vertexexpress"
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        use crate::channel::routes::{cg, local, pass, pv, responses_ws_to, xform};
        use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};
        let mut routes = vec![
            // Model list/get — served locally from a static model catalogue;
            // Vertex AI Express does not expose a standard model-listing
            // endpoint.
            local(ListModels, pv(P::Gemini)),
            local(ListModels, pv(P::Claude)),
            local(ListModels, pv(P::OpenAi)),
            local(GetModel, pv(P::Gemini)),
            local(GetModel, pv(P::Claude)),
            local(GetModel, pv(P::OpenAi)),
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
        ];
        routes.extend(responses_ws_to(cg(GeminiGenerateContent)));
        routes
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let api_key = common::resolve_api_key(&ctx)?;
        let query = auth::apply_query(
            allow_query_with_settings(ctx.query, DEFAULTS.forward_query, ctx.provider_settings),
            &api_key,
        );
        let path = default_request_path(ctx.path);
        let uri = common::resolve_uri(&ctx, &DEFAULTS, &path, query.as_deref())?;
        let headers = allow_headers_with_settings(
            ctx.headers,
            DEFAULTS.forward_headers,
            ctx.provider_settings,
        );
        let req = build_request(ctx.method, uri, headers, ctx.body)?;
        Ok(PreparedRequest::new(req))
    }

    fn shape_request(&self, body: Bytes, _headers: &mut http::HeaderMap, _ctx: &ShapeCtx) -> Bytes {
        shaping::with_json_body(body, gemini_genconfig::strip_store)
    }

    /// Normalize Gemini content responses to AI-Studio shape (citation rename,
    /// block-reason fix). Non-content ops and other kinds pass through.
    fn shape_response(&self, body: Bytes, ctx: &ShapeCtx) -> Bytes {
        if is_gemini_content(ctx) {
            vertex_normalize::normalize_vertex_response(body)
        } else {
            body
        }
    }

    /// Vertex AI Express exposes no model-listing endpoint; the admin model-pull
    /// reads this bundled Gemini-shaped catalogue instead of calling upstream.
    fn bundled_models(&self) -> Option<ModelCatalog> {
        Some(ModelCatalog {
            family: Provider::Gemini,
            body: Bytes::from_static(include_str!("models.gemini.json").as_bytes()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn default_content_paths_use_publisher_model_resources() {
        for verb in ["generateContent", "streamGenerateContent", "countTokens"] {
            let input = format!("/v1beta/models/gemini-test:{verb}");
            assert_eq!(
                default_request_path(&input),
                format!("/v1/publishers/google/models/gemini-test:{verb}")
            );
        }
        assert_eq!(
            default_request_path("/v1beta/models/gemini-test:embedContent"),
            "/v1beta/models/gemini-test:embedContent"
        );
    }

    #[test]
    fn shape_response_normalizes_gemini_content_only() {
        use crate::protocol::{Operation, OperationKey, Provider as P};

        let body = Bytes::from(
            json!({"promptFeedback": {"blockReason": "BLOCKED_REASON_UNSPECIFIED"}}).to_string(),
        );

        // Gemini content op → block reason normalized.
        let content_ctx = ShapeCtx {
            op: OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::GeminiGenerateContent,
            ),
            stream: false,
            status: http::StatusCode::OK,
            settings: &Value::Null,
        };
        let out = VertexExpressChannel.shape_response(body.clone(), &content_ctx);
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(
            v["promptFeedback"]["blockReason"],
            "BLOCK_REASON_UNSPECIFIED"
        );

        // Non-content op → untouched.
        let count_ctx = ShapeCtx {
            op: OperationKey::provider(Operation::CountTokens, P::Gemini),
            stream: false,
            status: http::StatusCode::OK,
            settings: &Value::Null,
        };
        let out2 = VertexExpressChannel.shape_response(body.clone(), &count_ctx);
        assert_eq!(out2, body);
    }
}
