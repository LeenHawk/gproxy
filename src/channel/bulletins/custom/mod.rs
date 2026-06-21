//! Custom (universal) channel — a generic passthrough to any OpenAI / Claude /
//! Gemini-compatible endpoint. `base_url` is REQUIRED (no baked default); the
//! auth header is chosen by the inbound protocol (see [`auth`]).

mod auth;

use crate::channel::bulletins::common::{self, ApiKeyDefaults};
use crate::channel::{Channel, ChannelError, PrepareCtx, PreparedRequest};
use crate::protocol::Provider;

const DEFAULTS: ApiKeyDefaults = ApiKeyDefaults {
    default_base_url: None, // base_url must be supplied in settings_json
    forward_headers: &[],
    forward_query: &[],
};

pub struct CustomChannel;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for CustomChannel {
    fn id(&self) -> &'static str {
        "custom"
    }

    fn provider_family(&self) -> Provider {
        Provider::OpenAi
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        // Universal transparent passthrough: every (operation, kind) cell the v1
        // custom channel served, mapped to v2 cells. v1 emitted all protocols ×
        // all ops as passthrough; the OpenAI-family protocols collapse to a
        // single provider/content cell each. WebSocket/Live, the *Stream* image
        // ops, the bare-`OpenAi` content cell, and GeminiNDJson have no v2
        // representation and are dropped.
        use crate::channel::routes::{cg, pass, pv};
        use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};
        vec![
            pass(ListModels, pv(P::OpenAi)),
            pass(ListModels, pv(P::Claude)),
            pass(ListModels, pv(P::Gemini)),
            pass(GetModel, pv(P::OpenAi)),
            pass(GetModel, pv(P::Claude)),
            pass(GetModel, pv(P::Gemini)),
            pass(CountTokens, pv(P::OpenAi)),
            pass(CountTokens, pv(P::Claude)),
            pass(CountTokens, pv(P::Gemini)),
            pass(GenerateContent, cg(OpenAiResponses)),
            pass(GenerateContent, cg(OpenAiChatCompletions)),
            pass(GenerateContent, cg(ClaudeMessages)),
            pass(GenerateContent, cg(GeminiGenerateContent)),
            pass(StreamGenerateContent, cg(OpenAiResponses)),
            pass(StreamGenerateContent, cg(OpenAiChatCompletions)),
            pass(StreamGenerateContent, cg(ClaudeMessages)),
            pass(StreamGenerateContent, cg(GeminiGenerateContent)),
            pass(CreateEmbedding, pv(P::OpenAi)),
            pass(CreateEmbedding, pv(P::Claude)),
            pass(CreateEmbedding, pv(P::Gemini)),
            pass(CreateImage, pv(P::OpenAi)),
            pass(CreateImage, pv(P::Claude)),
            pass(CreateImage, pv(P::Gemini)),
            pass(EditImage, pv(P::OpenAi)),
            pass(EditImage, pv(P::Claude)),
            pass(EditImage, pv(P::Gemini)),
            pass(CompactContent, pv(P::OpenAi)),
            pass(CompactContent, pv(P::Claude)),
            pass(CompactContent, pv(P::Gemini)),
        ]
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        // Decide the auth style from the inbound path BEFORE `ctx` is consumed.
        let proto = auth::detect(ctx.path);
        let (mut req, key) = common::build_request(ctx, &DEFAULTS)?;
        auth::apply(&mut req, &key, proto)?;
        Ok(PreparedRequest::new(req))
    }
}
