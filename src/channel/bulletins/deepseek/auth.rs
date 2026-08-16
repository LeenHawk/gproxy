//! DeepSeek auth + path selection.
//!
//! DeepSeek documents its Responses endpoint without GPROXY's public `/v1`
//! prefix and its Anthropic-compatible endpoint under `/anthropic/v1/messages`.
//! The Anthropic surface wants `x-api-key` (like the real Anthropic API);
//! everything else uses `Authorization: Bearer`.

use bytes::Bytes;
use http::Request;
use http::header::{HeaderName, HeaderValue};

use crate::channel::ChannelError;
use crate::channel::bulletins::common;

/// Inbound Claude-messages path. A passthrough `cg(ClaudeMessages)` request
/// arrives here verbatim; DeepSeek serves it from its Anthropic-compat surface.
const CLAUDE_MESSAGES_PATH: &str = "/v1/messages";
/// DeepSeek's Anthropic-compatible upstream path.
const ANTHROPIC_MESSAGES_PATH: &str = "/anthropic/v1/messages";
/// GPROXY's public OpenAI Responses path.
const OPENAI_RESPONSES_PATH: &str = "/v1/responses";
/// DeepSeek's documented Responses path.
const DEEPSEEK_RESPONSES_PATH: &str = "/responses";

/// Map GPROXY's public Claude Messages and OpenAI Responses paths to DeepSeek's
/// documented upstream paths; every other path is already accepted upstream.
pub(super) fn upstream_path(path: &str) -> &str {
    match path {
        CLAUDE_MESSAGES_PATH => ANTHROPIC_MESSAGES_PATH,
        OPENAI_RESPONSES_PATH => DEEPSEEK_RESPONSES_PATH,
        _ => path,
    }
}

/// Inject the credential for `path`: `x-api-key` on the Anthropic-compat
/// messages surface, `Authorization: Bearer` everywhere else.
pub(super) fn apply(req: &mut Request<Bytes>, path: &str, key: &str) -> Result<(), ChannelError> {
    if path == ANTHROPIC_MESSAGES_PATH {
        let v = HeaderValue::from_str(key)
            .map_err(|e| ChannelError::InvalidCredential(format!("bad api_key: {e}")))?;
        req.headers_mut()
            .insert(HeaderName::from_static("x-api-key"), v);
        Ok(())
    } else {
        common::inject_bearer(req, key)
    }
}
