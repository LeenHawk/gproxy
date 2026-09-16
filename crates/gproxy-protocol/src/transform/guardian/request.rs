use crate::{
    WireRequest,
    codec::{self, CodecLimits},
    transform::{Converted, Report, TransformError},
    wire::{
        DeclaredFields,
        claude::{content as cc, count_tokens as ct, generate_content as cg},
        gemini as g,
        openai::{chat as o, guardian as source, responses as r},
    },
};
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};

mod attachments;
mod binding;
mod claude;
mod controls;
mod evidence;
mod gemini;
mod media;
mod openai_chat;
mod openai_responses;
use attachments::attach_media;
use claude::prepare_claude_plan;
pub use claude::{prepare_claude, prepare_claude_with_limits};
use controls::*;
use evidence::*;
use gemini::prepare_gemini_plan;
pub use gemini::{prepare_gemini, prepare_gemini_with_limits};
use media::*;
use openai_chat::prepare_openai_chat_plan;
pub use openai_chat::{prepare_openai_chat, prepare_openai_chat_with_limits};
use openai_responses::prepare_openai_responses_plan;
pub use openai_responses::{prepare_openai_responses, prepare_openai_responses_with_limits};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardianOperation {
    Review,
    Classify,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardianRequestContext {
    pub target_model: String,
    pub max_tokens: i64,
    pub operation: GuardianOperation,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum GuardianDialectRequest {
    Claude(WireRequest<cg::GenerateContentRequestBody>),
    Gemini(WireRequest<g::GenerateContentRequestBody>),
    OpenAiChat(WireRequest<o::GenerateContentRequestBody>),
    OpenAiResponses(WireRequest<r::GenerateContentRequestBody>),
}

#[derive(Debug)]
pub struct GuardianPreparedRequest {
    operation: GuardianOperation,
    request: GuardianDialectRequest,
    source: source::GuardianRequestBody,
}

impl GuardianPreparedRequest {
    pub fn source(&self) -> &source::GuardianRequestBody {
        &self.source
    }
    pub fn operation(&self) -> GuardianOperation {
        self.operation
    }
    pub fn request(&self) -> &GuardianDialectRequest {
        &self.request
    }
    pub fn into_request(self) -> GuardianDialectRequest {
        self.request
    }
}

/// Destination API for resource-assisted Guardian preparation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardianTarget {
    Claude,
    Gemini,
    OpenAiChat,
    OpenAiResponses,
}

pub(super) fn request<T>(path: impl Into<String>, body: T) -> WireRequest<T> {
    WireRequest {
        method: http::Method::POST,
        path: path.into(),
        query: None,
        headers: http::HeaderMap::new(),
        body,
    }
}

pub fn preflight_request(
    request: &GuardianDialectRequest,
    limits: CodecLimits,
    max_bytes: u64,
) -> Result<(), TransformError> {
    let bytes = match request {
        GuardianDialectRequest::Claude(v) => codec::encode_json(&v.body, limits),
        GuardianDialectRequest::Gemini(v) => codec::encode_json(&v.body, limits),
        GuardianDialectRequest::OpenAiChat(v) => codec::encode_json(&v.body, limits),
        GuardianDialectRequest::OpenAiResponses(v) => codec::encode_json(&v.body, limits),
    }
    .map_err(|e| {
        TransformError::new(
            if e.kind() == codec::CodecErrorKind::Limit {
                crate::transform::TransformErrorKind::Limit
            } else {
                crate::transform::TransformErrorKind::InvalidInput
            },
            "guardian.request",
            e.to_string(),
        )
    })?;
    if bytes.len() as u64 > max_bytes {
        return Err(TransformError::new(
            crate::transform::TransformErrorKind::Limit,
            "guardian.request",
            "preflight byte cap exceeded",
        ));
    }
    Ok(())
}

pub(crate) fn prepare_unattached(
    input: source::GuardianRequestBody,
    target: GuardianTarget,
    context: GuardianRequestContext,
    limits: CodecLimits,
) -> Result<(Converted<GuardianPreparedRequest>, Vec<Media>), TransformError> {
    let prepared = match target {
        GuardianTarget::Claude => prepare_claude_plan(input, context, limits, false),
        GuardianTarget::Gemini => prepare_gemini_plan(input, context, limits, false),
        GuardianTarget::OpenAiChat => prepare_openai_chat_plan(input, context, limits, false),
        GuardianTarget::OpenAiResponses => {
            prepare_openai_responses_plan(input, context, limits, false)
        }
    }?;
    let media = media(prepared.value.source())?;
    Ok((prepared, media))
}

pub(crate) fn finish_attachments(
    prepared: &mut GuardianPreparedRequest,
    media: &[Media],
    limits: CodecLimits,
) -> Result<(), TransformError> {
    attach_media(&mut prepared.request, media)?;
    preflight_request(&prepared.request, limits, limits.max_body_bytes)
}
