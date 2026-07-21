pub(super) fn table() -> crate::channel::routes::RouteList {
    use crate::channel::routes::{cg, local, pass, responses_ws_to, xform};
    use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};

    let mut routes = vec![
        local(ListModels, crate::channel::routes::pv(P::OpenAi)),
        local(ListModels, crate::channel::routes::pv(P::Claude)),
        local(ListModels, crate::channel::routes::pv(P::Gemini)),
        local(GetModel, crate::channel::routes::pv(P::OpenAi)),
        local(GetModel, crate::channel::routes::pv(P::Claude)),
        local(GetModel, crate::channel::routes::pv(P::Gemini)),
        local(CountTokens, crate::channel::routes::pv(P::OpenAi)),
        local(CountTokens, crate::channel::routes::pv(P::Claude)),
        local(CountTokens, crate::channel::routes::pv(P::Gemini)),
        xform(
            GenerateContent,
            cg(OpenAiChatCompletions),
            StreamGenerateContent,
            cg(OpenAiChatCompletions),
        ),
        xform(
            GenerateContent,
            cg(OpenAiResponses),
            StreamGenerateContent,
            cg(OpenAiChatCompletions),
        ),
        xform(
            GenerateContent,
            cg(ClaudeMessages),
            StreamGenerateContent,
            cg(OpenAiChatCompletions),
        ),
        xform(
            GenerateContent,
            cg(GeminiGenerateContent),
            StreamGenerateContent,
            cg(OpenAiChatCompletions),
        ),
        pass(StreamGenerateContent, cg(OpenAiChatCompletions)),
        xform(
            StreamGenerateContent,
            cg(OpenAiResponses),
            StreamGenerateContent,
            cg(OpenAiChatCompletions),
        ),
        xform(
            StreamGenerateContent,
            cg(ClaudeMessages),
            StreamGenerateContent,
            cg(OpenAiChatCompletions),
        ),
        xform(
            StreamGenerateContent,
            cg(GeminiGenerateContent),
            StreamGenerateContent,
            cg(OpenAiChatCompletions),
        ),
    ];
    routes.extend(responses_ws_to(cg(OpenAiChatCompletions)));
    routes
}
