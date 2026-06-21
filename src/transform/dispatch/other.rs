//! Non-content dispatch arms (M2.5): count_tokens, models, embeddings,
//! images, and compact. None of these groups stream; models requests are
//! GET/no-body and pass through unchanged.

use super::{not_wired, run, run_ok};
use crate::protocol::Operation;
use crate::transform::{TransformContext, TransformError, TransformPair};
use crate::transform::{compact, count_tokens as ct, embeddings, images, models};

/// Whether this pair has at least one wired direction below.
pub(super) fn is_wired(pair: TransformPair) -> bool {
    use TransformPair as P;
    matches!(
        pair,
        // count_tokens
        P::OpenAiToClaudeCountTokens
            | P::ClaudeToOpenAiCountTokens
            | P::OpenAiToGeminiCountTokens
            | P::GeminiToOpenAiCountTokens
            | P::ClaudeToGeminiCountTokens
            | P::GeminiToClaudeCountTokens
            // models (each serves ListModels and GetModel)
            | P::OpenAiToClaudeModels
            | P::ClaudeToOpenAiModels
            | P::OpenAiToGeminiModels
            | P::GeminiToOpenAiModels
            | P::ClaudeToGeminiModels
            | P::GeminiToClaudeModels
            // embeddings (single-embed form)
            | P::OpenAiToGeminiEmbeddings
            | P::GeminiToOpenAiEmbeddings
            // images
            | P::OpenAiCreateImageToGemini
            | P::GeminiToOpenAiCreateImage
            | P::OpenAiCreateImageToOpenAiResponses
            | P::OpenAiEditImageToGemini
            | P::GeminiToOpenAiEditImage
            // compact (request-only and response-only pairs are each used in
            // exactly that direction by the reverse-pair convention)
            | P::OpenAiToClaudeCompact
            | P::ClaudeToOpenAiCompact
            | P::OpenAiResponsesToOpenAiCompact
            | P::OpenAiCompactToGemini
            | P::GeminiToOpenAiCompact
            | P::OpenAiCompactToOpenAiChat
            | P::OpenAiChatToOpenAiCompact
    )
}

pub(super) fn request_bytes(
    pair: TransformPair,
    ctx: &TransformContext,
    body: &[u8],
) -> Result<Vec<u8>, TransformError> {
    use TransformPair as P;
    match pair {
        // count_tokens
        P::OpenAiToClaudeCountTokens => run(ct::openai_to_claude::request, ctx, body),
        P::ClaudeToOpenAiCountTokens => run(ct::claude_to_openai::request, ctx, body),
        P::OpenAiToGeminiCountTokens => run(ct::openai_to_gemini::request, ctx, body),
        P::GeminiToOpenAiCountTokens => run(ct::gemini_to_openai::request, ctx, body),
        P::ClaudeToGeminiCountTokens => run(ct::claude_to_gemini::request, ctx, body),
        P::GeminiToClaudeCountTokens => run(ct::gemini_to_claude::request, ctx, body),
        // models: GET/no-body requests — pass the (empty) body through
        // unchanged; parameters travel in path/query, not the body.
        P::OpenAiToClaudeModels
        | P::ClaudeToOpenAiModels
        | P::OpenAiToGeminiModels
        | P::GeminiToOpenAiModels
        | P::ClaudeToGeminiModels
        | P::GeminiToClaudeModels => Ok(body.to_vec()),
        // embeddings (single-embed form; batch shapes are not wired in M2.5)
        P::OpenAiToGeminiEmbeddings => {
            run(embeddings::single::openai_to_gemini::request, ctx, body)
        }
        P::GeminiToOpenAiEmbeddings => {
            run(embeddings::single::gemini_to_openai::request, ctx, body)
        }
        // images
        P::OpenAiCreateImageToGemini => run(images::create::openai_to_gemini::request, ctx, body),
        P::GeminiToOpenAiCreateImage => run(images::create::gemini_to_openai::request, ctx, body),
        P::OpenAiCreateImageToOpenAiResponses => run(
            images::create::openai_to_openai_responses::request,
            ctx,
            body,
        ),
        P::OpenAiEditImageToGemini => run(images::edit::openai_to_gemini::request, ctx, body),
        P::GeminiToOpenAiEditImage => run(images::edit::gemini_to_openai::request, ctx, body),
        // compact
        P::OpenAiToClaudeCompact => run(compact::openai_to_claude::request, ctx, body),
        P::ClaudeToOpenAiCompact => run(compact::claude_to_openai::request, ctx, body),
        P::OpenAiResponsesToOpenAiCompact => run(
            compact::openai_responses_to_openai_compact::request,
            ctx,
            body,
        ),
        P::OpenAiCompactToGemini => run(compact::openai_compact_to_gemini::request, ctx, body),
        P::OpenAiCompactToOpenAiChat => {
            run(compact::openai_compact_to_openai_chat::request, ctx, body)
        }
        other => Err(not_wired(other)),
    }
}

pub(super) fn response_bytes(
    pair: TransformPair,
    ctx: &TransformContext,
    body: &[u8],
) -> Result<Vec<u8>, TransformError> {
    use TransformPair as P;
    match pair {
        // count_tokens (responses are infallible)
        P::OpenAiToClaudeCountTokens => run_ok(ct::openai_to_claude::response, ctx, body),
        P::ClaudeToOpenAiCountTokens => run_ok(ct::claude_to_openai::response, ctx, body),
        P::OpenAiToGeminiCountTokens => run_ok(ct::openai_to_gemini::response, ctx, body),
        P::GeminiToOpenAiCountTokens => run_ok(ct::gemini_to_openai::response, ctx, body),
        P::ClaudeToGeminiCountTokens => run_ok(ct::claude_to_gemini::response, ctx, body),
        P::GeminiToClaudeCountTokens => run_ok(ct::gemini_to_claude::response, ctx, body),
        // models: each pair serves both list and get; split on operation
        P::OpenAiToClaudeModels
        | P::ClaudeToOpenAiModels
        | P::OpenAiToGeminiModels
        | P::GeminiToOpenAiModels
        | P::ClaudeToGeminiModels
        | P::GeminiToClaudeModels => models_response(pair, ctx, body),
        // embeddings (single-embed form)
        P::OpenAiToGeminiEmbeddings => {
            run(embeddings::single::openai_to_gemini::response, ctx, body)
        }
        P::GeminiToOpenAiEmbeddings => {
            run(embeddings::single::gemini_to_openai::response, ctx, body)
        }
        // images
        P::OpenAiCreateImageToGemini => run(images::create::openai_to_gemini::response, ctx, body),
        P::GeminiToOpenAiCreateImage => run(images::create::gemini_to_openai::response, ctx, body),
        P::OpenAiCreateImageToOpenAiResponses => run(
            images::create::openai_to_openai_responses::response,
            ctx,
            body,
        ),
        P::OpenAiEditImageToGemini => run(images::edit::openai_to_gemini::response, ctx, body),
        P::GeminiToOpenAiEditImage => run(images::edit::gemini_to_openai::response, ctx, body),
        // compact
        P::OpenAiToClaudeCompact => run_ok(compact::openai_to_claude::response, ctx, body),
        P::ClaudeToOpenAiCompact => run_ok(compact::claude_to_openai::response, ctx, body),
        P::OpenAiResponsesToOpenAiCompact => run_ok(
            compact::openai_responses_to_openai_compact::response,
            ctx,
            body,
        ),
        P::GeminiToOpenAiCompact => run(compact::gemini_to_openai_compact::response, ctx, body),
        P::OpenAiChatToOpenAiCompact => {
            run(compact::openai_chat_to_openai_compact::response, ctx, body)
        }
        other => Err(not_wired(other)),
    }
}

/// Models response dispatch: the pair identifies the provider direction, the
/// source operation selects the list vs get shape.
fn models_response(
    pair: TransformPair,
    ctx: &TransformContext,
    body: &[u8],
) -> Result<Vec<u8>, TransformError> {
    use TransformPair as P;
    use models::{get, list};
    match (ctx.source.operation, pair) {
        (Operation::ListModels, P::OpenAiToClaudeModels) => {
            run(list::openai_to_claude::response, ctx, body)
        }
        (Operation::ListModels, P::ClaudeToOpenAiModels) => {
            run(list::claude_to_openai::response, ctx, body)
        }
        (Operation::ListModels, P::OpenAiToGeminiModels) => {
            run(list::openai_to_gemini::response, ctx, body)
        }
        (Operation::ListModels, P::GeminiToOpenAiModels) => {
            run(list::gemini_to_openai::response, ctx, body)
        }
        (Operation::ListModels, P::ClaudeToGeminiModels) => {
            run(list::claude_to_gemini::response, ctx, body)
        }
        (Operation::ListModels, P::GeminiToClaudeModels) => {
            run_ok(list::gemini_to_claude::response, ctx, body)
        }
        (Operation::GetModel, P::OpenAiToClaudeModels) => {
            run(get::openai_to_claude::response, ctx, body)
        }
        (Operation::GetModel, P::ClaudeToOpenAiModels) => {
            run(get::claude_to_openai::response, ctx, body)
        }
        (Operation::GetModel, P::OpenAiToGeminiModels) => {
            run(get::openai_to_gemini::response, ctx, body)
        }
        (Operation::GetModel, P::GeminiToOpenAiModels) => {
            run_ok(get::gemini_to_openai::response, ctx, body)
        }
        (Operation::GetModel, P::ClaudeToGeminiModels) => {
            run(get::claude_to_gemini::response, ctx, body)
        }
        (Operation::GetModel, P::GeminiToClaudeModels) => {
            run_ok(get::gemini_to_claude::response, ctx, body)
        }
        (op, pair) => Err(TransformError::InvalidInput {
            reason: format!("models pair {pair:?} with non-models operation {op:?}"),
        }),
    }
}
