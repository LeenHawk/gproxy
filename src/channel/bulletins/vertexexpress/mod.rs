//! Vertex AI Express channel — api key in the `?key=` query param, default
//! `https://aiplatform.googleapis.com`.

mod auth;

use bytes::Bytes;

use crate::channel::bulletins::common::{self, ApiKeyDefaults};
use crate::channel::http_util::{allow_headers, allow_query, build_request, join_url};
use crate::channel::shaping::vertex_normalize;
use crate::channel::{Channel, ChannelError, PrepareCtx, PreparedRequest, ShapeCtx};
use crate::protocol::{ContentGenerationKind, OperationKind, Provider};

const DEFAULTS: ApiKeyDefaults = ApiKeyDefaults {
    default_base_url: Some("https://aiplatform.googleapis.com"),
    forward_headers: &[],
    forward_query: &["alt"],
};

/// Whether this op is a Gemini content-generation call (the only response shape
/// VertexExpress normalizes; everything else passes through untouched).
fn is_gemini_content(ctx: &ShapeCtx) -> bool {
    matches!(
        ctx.op.kind,
        OperationKind::ContentGeneration(ContentGenerationKind::GeminiGenerateContent)
    )
}

pub struct VertexExpressChannel;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for VertexExpressChannel {
    fn id(&self) -> &'static str {
        "vertexexpress"
    }

    fn provider_family(&self) -> Provider {
        Provider::Gemini
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        use crate::channel::routes::{cg, local, pass, pv, xform};
        use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};
        vec![
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
        ]
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let base_url = common::resolve_base_url(&ctx, &DEFAULTS)?;
        let api_key = common::resolve_api_key(&ctx)?;
        let query = auth::apply_query(allow_query(ctx.query, DEFAULTS.forward_query), &api_key);
        let uri = join_url(&base_url, ctx.path, query.as_deref())?;
        let headers = allow_headers(ctx.headers, DEFAULTS.forward_headers);
        let req = build_request(ctx.method, uri, headers, ctx.body)?;
        Ok(PreparedRequest::new(req))
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
    fn bundled_models(&self) -> Option<Bytes> {
        Some(Bytes::from_static(
            include_str!("models.gemini.json").as_bytes(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

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
            enable_magic_cache: false,
            enable_claude_fable_fallback: false,
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
            enable_magic_cache: false,
            enable_claude_fable_fallback: false,
        };
        let out2 = VertexExpressChannel.shape_response(body.clone(), &count_ctx);
        assert_eq!(out2, body);
    }
}
