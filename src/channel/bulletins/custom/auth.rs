//! Custom auth: protocol-driven — Bearer for OpenAI-shaped paths, `x-api-key`
//! (+ `anthropic-version`) for Claude, `x-goog-api-key` for Gemini.

use bytes::Bytes;
use http::HeaderName;
use http::Request;

use crate::channel::ChannelError;
use crate::channel::bulletins::common;

const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Inbound protocol guessed from the request path.
#[derive(Clone, Copy)]
pub(super) enum Proto {
    OpenAi,
    Claude,
    Gemini,
}

/// Classify the inbound path into a protocol for auth selection.
pub(super) fn detect(path: &str) -> Proto {
    if path.contains("/messages") {
        Proto::Claude
    } else if path.starts_with("/v1beta")
        || path.contains(":generateContent")
        || path.contains(":streamGenerateContent")
        || path.contains(":countTokens")
    {
        Proto::Gemini
    } else {
        Proto::OpenAi
    }
}

pub(super) fn apply(req: &mut Request<Bytes>, key: &str, proto: Proto) -> Result<(), ChannelError> {
    match proto {
        Proto::OpenAi => common::inject_bearer(req, key),
        Proto::Claude => {
            common::inject_header(req, HeaderName::from_static("x-api-key"), key)?;
            common::inject_static(
                req,
                HeaderName::from_static("anthropic-version"),
                ANTHROPIC_VERSION,
            );
            Ok(())
        }
        Proto::Gemini => common::inject_header(req, HeaderName::from_static("x-goog-api-key"), key),
    }
}
