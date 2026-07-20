//! Kiro operation routing.

use crate::channel::routes::{RouteList, cg, local, pass, pv, responses_ws_to, xform};
use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};

pub(super) fn table() -> RouteList {
    let mut routes = vec![
        pass(ListModels, pv(P::OpenAi)),
        xform(ListModels, pv(P::Claude), ListModels, pv(P::OpenAi)),
        xform(ListModels, pv(P::Gemini), ListModels, pv(P::OpenAi)),
        local(CountTokens, pv(P::OpenAi)),
        local(CountTokens, pv(P::Claude)),
        local(CountTokens, pv(P::Gemini)),
        // Kiro's upstream only speaks the AWS event stream. Non-stream clients
        // are collapsed by the pipeline after using a streaming upstream.
        xform(
            GenerateContent,
            cg(OpenAiResponses),
            StreamGenerateContent,
            cg(OpenAiResponses),
        ),
        xform(
            GenerateContent,
            cg(OpenAiChatCompletions),
            StreamGenerateContent,
            cg(OpenAiResponses),
        ),
        xform(
            GenerateContent,
            cg(ClaudeMessages),
            StreamGenerateContent,
            cg(OpenAiResponses),
        ),
        xform(
            GenerateContent,
            cg(GeminiGenerateContent),
            StreamGenerateContent,
            cg(OpenAiResponses),
        ),
        pass(StreamGenerateContent, cg(OpenAiResponses)),
        xform(
            StreamGenerateContent,
            cg(OpenAiChatCompletions),
            StreamGenerateContent,
            cg(OpenAiResponses),
        ),
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
    ];
    routes.extend(responses_ws_to(cg(OpenAiResponses)));
    routes
}
