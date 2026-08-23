mod buffered;
mod chat;
pub(crate) mod common;
mod responses;
mod tools;

use bytes::Bytes;

use crate::TransformError;

pub(crate) fn chat_to_claude(
    body: Bytes,
    model: &str,
    stream: bool,
) -> Result<Bytes, TransformError> {
    chat::request_to_claude(body, model, stream)
}

pub(crate) fn claude_to_chat(
    body: Bytes,
    model: &str,
    stream: bool,
) -> Result<Bytes, TransformError> {
    chat::request_to_chat(body, model, stream)
}

pub(crate) fn responses_to_claude(
    body: Bytes,
    model: &str,
    stream: bool,
) -> Result<Bytes, TransformError> {
    responses::request_to_claude(body, model, stream)
}

pub(crate) fn claude_to_responses(
    body: Bytes,
    model: &str,
    stream: bool,
) -> Result<Bytes, TransformError> {
    responses::request_to_responses(body, model, stream)
}

pub(crate) use buffered::{
    chat_to_claude_response, claude_to_chat_response, claude_to_responses_response,
    responses_to_claude_response,
};
