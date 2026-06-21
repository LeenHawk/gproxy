//! Google AI Studio (Gemini) channel — api key in the `?key=` query param,
//! default `https://generativelanguage.googleapis.com`.

mod auth;

use crate::channel::bulletins::common::{self, ApiKeyDefaults};
use crate::channel::http_util::{allow_headers, allow_query, build_request, join_url};
use crate::channel::{Channel, ChannelError, PrepareCtx, PreparedRequest};
use crate::protocol::Provider;

const DEFAULTS: ApiKeyDefaults = ApiKeyDefaults {
    default_base_url: Some("https://generativelanguage.googleapis.com"),
    forward_headers: &[],
    forward_query: &["alt"],
};

pub struct AiStudioChannel;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for AiStudioChannel {
    fn id(&self) -> &'static str {
        "aistudio"
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
            pass(ListModels, pv(P::OpenAi)),
            pass(GetModel, pv(P::Gemini)),
            xform(GetModel, pv(P::Claude), GetModel, pv(P::Gemini)),
            pass(GetModel, pv(P::OpenAi)),
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
            pass(GenerateContent, cg(OpenAiChatCompletions)),
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
            pass(StreamGenerateContent, cg(OpenAiChatCompletions)),
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
}
