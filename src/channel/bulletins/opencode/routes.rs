//! Declared routing surfaces for the two OpenCode tiers.
//!
//! The gateway converts between wire formats server-side, so every surface it
//! exposes is a plain passthrough for this channel regardless of which upstream
//! model the request names. The tiers differ in one cell: Zen serves Gemini
//! natively at `/models/{model}:…`, Go has no Gemini route at all and folds it
//! into chat completions.
//!
//! `GetModel` and `CountTokens` are local: the gateway publishes a list at
//! `GET {base}/models` but has no per-model GET and no token-count endpoint.

use crate::channel::routes::{RouteList, cg, local, pass, pv, responses_ws_to, xform};
use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};

/// Cells shared by both tiers: the model list, the local ops, and the three
/// surfaces every tier serves natively.
fn shared() -> RouteList {
    let mut routes = vec![
        // ── Model list: one OpenAI-shaped list endpoint feeds all families ──
        pass(ListModels, pv(P::OpenAi)),
        xform(ListModels, pv(P::Claude), ListModels, pv(P::OpenAi)),
        xform(ListModels, pv(P::Gemini), ListModels, pv(P::OpenAi)),
        // ── Local: no upstream get-model / count-tokens endpoint ──
        local(GetModel, pv(P::OpenAi)),
        local(GetModel, pv(P::Claude)),
        local(GetModel, pv(P::Gemini)),
        local(CountTokens, pv(P::OpenAi)),
        local(CountTokens, pv(P::Claude)),
        local(CountTokens, pv(P::Gemini)),
        // ── Content: native on both tiers ──
        pass(GenerateContent, cg(OpenAiChatCompletions)),
        pass(GenerateContent, cg(OpenAiResponses)),
        pass(GenerateContent, cg(ClaudeMessages)),
        pass(StreamGenerateContent, cg(OpenAiChatCompletions)),
        pass(StreamGenerateContent, cg(OpenAiResponses)),
        pass(StreamGenerateContent, cg(ClaudeMessages)),
    ];
    routes.extend(responses_ws_to(cg(OpenAiResponses)));
    routes
}

/// Zen: full curated catalogue, Gemini served natively.
#[cfg(feature = "channel-opencodezen")]
pub(super) fn zen() -> RouteList {
    let mut routes = shared();
    routes.extend([
        pass(GenerateContent, cg(GeminiGenerateContent)),
        pass(StreamGenerateContent, cg(GeminiGenerateContent)),
    ]);
    routes
}

/// Go: open-weights subset, no Gemini surface — Gemini clients are converted to
/// chat completions instead of being refused.
#[cfg(feature = "channel-opencodego")]
pub(super) fn go() -> RouteList {
    let mut routes = shared();
    routes.extend([
        xform(
            GenerateContent,
            cg(GeminiGenerateContent),
            GenerateContent,
            cg(OpenAiChatCompletions),
        ),
        xform(
            StreamGenerateContent,
            cg(GeminiGenerateContent),
            StreamGenerateContent,
            cg(OpenAiChatCompletions),
        ),
    ]);
    routes
}
