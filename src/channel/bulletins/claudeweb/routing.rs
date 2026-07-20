//! Claude Web protocol routing.

pub(super) fn table() -> crate::channel::routes::RouteList {
    use crate::channel::routes::{cg, local, responses_ws_to, xform};
    use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};

    let mut routes = vec![
        local(ListModels, crate::channel::routes::pv(P::Claude)),
        local(ListModels, crate::channel::routes::pv(P::OpenAi)),
        local(ListModels, crate::channel::routes::pv(P::Gemini)),
        local(GetModel, crate::channel::routes::pv(P::Claude)),
        local(GetModel, crate::channel::routes::pv(P::OpenAi)),
        local(GetModel, crate::channel::routes::pv(P::Gemini)),
        local(CountTokens, crate::channel::routes::pv(P::Claude)),
        local(CountTokens, crate::channel::routes::pv(P::OpenAi)),
        local(CountTokens, crate::channel::routes::pv(P::Gemini)),
        xform(
            GenerateContent,
            cg(ClaudeMessages),
            StreamGenerateContent,
            cg(ClaudeMessages),
        ),
        xform(
            GenerateContent,
            cg(OpenAiChatCompletions),
            StreamGenerateContent,
            cg(ClaudeMessages),
        ),
        xform(
            GenerateContent,
            cg(OpenAiResponses),
            StreamGenerateContent,
            cg(ClaudeMessages),
        ),
        xform(
            GenerateContent,
            cg(GeminiGenerateContent),
            StreamGenerateContent,
            cg(ClaudeMessages),
        ),
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
        crate::channel::routes::pass(StreamGenerateContent, cg(ClaudeMessages)),
    ];
    routes.extend(responses_ws_to(cg(ClaudeMessages)));
    routes
}
