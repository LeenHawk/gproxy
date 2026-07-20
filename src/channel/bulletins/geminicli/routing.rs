//! Gemini CLI protocol routing.

pub(super) fn table() -> crate::channel::routes::RouteList {
    use crate::channel::routes::{cg, pass, pv, responses_ws_to, xform};
    use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};

    let mut routes = vec![
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
    ];
    routes.extend(responses_ws_to(cg(GeminiGenerateContent)));
    routes
}
