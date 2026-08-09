//! OpenCode Zen / Go auth + upstream path selection.
//!
//! One gateway host serves four wire surfaces, and each surface reads the SAME
//! console API key from a different header: `Authorization: Bearer` on the
//! OpenAI chat/responses surfaces, `x-api-key` (+ `anthropic-version`) on
//! `/messages`, `x-goog-api-key` on the Gemini `/models/{model}:…` surface.
//!
//! Both the path and the header key off the ROUTED cell (`PrepareCtx::op`)
//! rather than the inbound path: a transformed candidate reaches `prepare` with
//! the downstream client's original path, which says nothing about the surface
//! this channel is about to call.

use std::borrow::Cow;

use bytes::Bytes;
use http::Request;
use http::header::HeaderName;

use crate::channel::ChannelError;
use crate::channel::bulletins::common;
use crate::protocol::{ContentGenerationKind, OperationKey, OperationKind};

const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Upstream path under the tier base URL for one routed cell.
///
/// Only the cells a tier declares in its routing table reach here; provider-kind
/// cells are the model list (`GetModel` and `CountTokens` are served locally,
/// because the gateway has no per-model GET and no token-count endpoint).
pub(super) fn upstream_path(
    op: OperationKey,
    stream: bool,
    model: &str,
) -> Result<Cow<'static, str>, ChannelError> {
    use ContentGenerationKind as C;

    let OperationKind::ContentGeneration(kind) = op.kind() else {
        return Ok(Cow::Borrowed("/models"));
    };
    Ok(match kind {
        C::OpenAiChatCompletions => Cow::Borrowed("/chat/completions"),
        C::OpenAiResponses | C::OpenAiResponsesWebSocket => Cow::Borrowed("/responses"),
        C::ClaudeMessages => Cow::Borrowed("/messages"),
        // The gateway reads the model from the path segment and keys streaming
        // off the verb suffix, exactly like the Gemini API it mirrors.
        C::GeminiGenerateContent => {
            let verb = if stream {
                "streamGenerateContent"
            } else {
                "generateContent"
            };
            Cow::Owned(format!("/models/{model}:{verb}"))
        }
        _ => return Err(ChannelError::Unsupported("opencode content kind")),
    })
}

/// Inject the console API key in the header the routed surface expects.
pub(super) fn apply(
    req: &mut Request<Bytes>,
    op: OperationKey,
    key: &str,
) -> Result<(), ChannelError> {
    match op.kind() {
        OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages) => {
            common::inject_header(req, HeaderName::from_static("x-api-key"), key)?;
            common::inject_static(
                req,
                HeaderName::from_static("anthropic-version"),
                ANTHROPIC_VERSION,
            );
            Ok(())
        }
        OperationKind::ContentGeneration(ContentGenerationKind::GeminiGenerateContent) => {
            common::inject_header(req, HeaderName::from_static("x-goog-api-key"), key)
        }
        _ => common::inject_bearer(req, key),
    }
}
