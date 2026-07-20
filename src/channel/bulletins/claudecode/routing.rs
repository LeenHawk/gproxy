//! Claude Code protocol routing.

pub(super) fn table() -> crate::channel::routes::RouteList {
    use crate::channel::routes::{cg, pass, pv, responses_ws_to, xform};
    use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};

    let mut routes = vec![
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
    ];
    routes.extend(responses_ws_to(cg(ClaudeMessages)));
    routes
}
