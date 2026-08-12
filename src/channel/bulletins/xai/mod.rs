//! xAI channel — official API-key access through `https://api.x.ai`.

mod auth;

#[cfg(test)]
mod tests;

use crate::channel::bulletins::common::{self, ApiKeyDefaults};
use crate::channel::{Channel, ChannelError, PrepareCtx, PreparedRequest};

const DEFAULTS: ApiKeyDefaults = ApiKeyDefaults {
    default_base_url: Some("https://api.x.ai"),
    forward_headers: &["x-grok-conv-id"],
    forward_query: &[],
};

pub struct XaiChannel;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for XaiChannel {
    fn id(&self) -> &'static str {
        "xai"
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        use crate::channel::routes::{cg, local, pass, pv, responses_ws_to, xform};
        use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};
        let mut routes = vec![
            pass(ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Claude), ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Gemini), ListModels, pv(P::OpenAi)),
            pass(GetModel, pv(P::OpenAi)),
            xform(GetModel, pv(P::Claude), GetModel, pv(P::OpenAi)),
            xform(GetModel, pv(P::Gemini), GetModel, pv(P::OpenAi)),
            local(CountTokens, pv(P::OpenAi)),
            local(CountTokens, pv(P::Claude)),
            local(CountTokens, pv(P::Gemini)),
            pass(GenerateContent, cg(OpenAiResponses)),
            pass(GenerateContent, cg(OpenAiChatCompletions)),
            xform(
                GenerateContent,
                cg(ClaudeMessages),
                GenerateContent,
                cg(OpenAiResponses),
            ),
            xform(
                GenerateContent,
                cg(GeminiGenerateContent),
                GenerateContent,
                cg(OpenAiResponses),
            ),
            pass(StreamGenerateContent, cg(OpenAiResponses)),
            pass(StreamGenerateContent, cg(OpenAiChatCompletions)),
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
            pass(CreateImage, pv(P::OpenAi)),
            pass(EditImage, pv(P::OpenAi)),
            pass(CompactContent, pv(P::OpenAi)),
        ];
        routes.extend(responses_ws_to(cg(OpenAiResponses)));
        routes
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let (mut req, key) = common::build_request(ctx, &DEFAULTS)?;
        auth::apply(&mut req, &key)?;
        Ok(PreparedRequest::new(req))
    }
}
