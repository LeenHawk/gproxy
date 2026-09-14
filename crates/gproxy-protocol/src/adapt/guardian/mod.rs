//! Host-facing Guardian invocation. One prepared target request is sent once;
//! non-2xx responses retain their complete original body and no retry is done.

use crate::{
    HttpBody, WireResponse,
    adapt::{JsonInvocation, invoke_json},
    capability::Upstream,
    codec::CodecLimits,
    transform::{
        TransformError,
        guardian::{
            self, GuardianClassifyResult, GuardianDialectRequest, GuardianExtraction,
            GuardianOperation, GuardianPreparedRequest, GuardianReviewResult,
        },
    },
};

#[derive(Debug, Clone, Copy)]
pub struct GuardianLimits {
    pub codec: CodecLimits,
    pub max_request_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardianPreflight {
    pub operation: GuardianOperation,
    pub model: String,
    pub max_bytes: u64,
}

impl GuardianPreflight {
    pub fn validate(&self) -> Result<(), crate::capability::CapabilityError> {
        if self.model.trim().is_empty() || self.max_bytes == 0 {
            return Err(crate::capability::CapabilityError::new(
                crate::capability::CapabilityErrorKind::Invalid,
                crate::capability::CapabilityErrorStage::Start,
                "invalid Guardian preflight",
            ));
        }
        Ok(())
    }
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum GuardianReviewInvocation {
    Success {
        result: GuardianReviewResult,
        native: GuardianNativeResponse,
    },
    Rejected(WireResponse<HttpBody>),
    Invalid {
        error: TransformError,
        native: GuardianNativeResponse,
    },
}
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum GuardianClassifyInvocation {
    Success {
        result: GuardianClassifyResult,
        native: GuardianNativeResponse,
    },
    Rejected(WireResponse<HttpBody>),
    Invalid {
        error: TransformError,
        native: GuardianNativeResponse,
    },
}

/// The validated semantic label is paired with the complete generated target
/// response. Hosts can use this provenance to map usage/IDs and synthesize the
/// native Responses SSE lifecycle without treating the label as a bare JSON
/// replacement for the client response.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum GuardianNativeResponse {
    Claude(WireResponse<crate::wire::claude::generate_content::GenerateContentResponseBody>),
    Gemini(WireResponse<crate::wire::gemini::GenerateContentResponseBody>),
    Chat(WireResponse<crate::wire::openai::chat::response::GenerateContentResponseBody>),
    Responses(WireResponse<crate::wire::openai::responses::GenerateContentResponseBody>),
}

pub async fn review<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    request: GuardianPreparedRequest,
    limits: GuardianLimits,
) -> Result<GuardianReviewInvocation, TransformError> {
    if request.operation() != GuardianOperation::Review {
        return Err(TransformError::new(
            crate::transform::TransformErrorKind::Conflict,
            "guardian.operation",
            "review received a classify request",
        ));
    }
    invoke(
        upstream,
        target,
        request.into_request(),
        GuardianOperation::Review,
        limits,
    )
    .await
    .map(map_review)
}

pub async fn classify<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    request: GuardianPreparedRequest,
    limits: GuardianLimits,
) -> Result<GuardianClassifyInvocation, TransformError> {
    if request.operation() != GuardianOperation::Classify {
        return Err(TransformError::new(
            crate::transform::TransformErrorKind::Conflict,
            "guardian.operation",
            "classify received a review request",
        ));
    }
    invoke(
        upstream,
        target,
        request.into_request(),
        GuardianOperation::Classify,
        limits,
    )
    .await
    .map(map_classify)
}

async fn invoke<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    request: GuardianDialectRequest,
    operation: GuardianOperation,
    limits: GuardianLimits,
) -> Result<Extracted, TransformError> {
    if limits.max_request_bytes == 0 {
        return Err(TransformError::new(
            crate::transform::TransformErrorKind::Limit,
            "guardian.request",
            "preflight byte cap must be positive",
        ));
    }
    guardian::preflight_request(&request, limits.codec, limits.max_request_bytes)?;
    match request {
        GuardianDialectRequest::Claude(request) => {
            match invoke_json(upstream, target, request, limits.codec).await? {
                JsonInvocation::Success(response) => {
                    Ok(GuardianExtractionInvocation::Claude(response))
                }
                JsonInvocation::Rejected(response) => {
                    Ok(GuardianExtractionInvocation::Rejected(response))
                }
            }
        }
        GuardianDialectRequest::Gemini(request) => {
            match invoke_json(upstream, target, request, limits.codec).await? {
                JsonInvocation::Success(response) => {
                    Ok(GuardianExtractionInvocation::Gemini(response))
                }
                JsonInvocation::Rejected(response) => {
                    Ok(GuardianExtractionInvocation::Rejected(response))
                }
            }
        }
        GuardianDialectRequest::OpenAiChat(request) => {
            match invoke_json(upstream, target, request, limits.codec).await? {
                JsonInvocation::Success(response) => {
                    Ok(GuardianExtractionInvocation::Chat(response))
                }
                JsonInvocation::Rejected(response) => {
                    Ok(GuardianExtractionInvocation::Rejected(response))
                }
            }
        }
        GuardianDialectRequest::OpenAiResponses(request) => {
            match invoke_json(upstream, target, request, limits.codec).await? {
                JsonInvocation::Success(response) => {
                    Ok(GuardianExtractionInvocation::Responses(response))
                }
                JsonInvocation::Rejected(response) => {
                    Ok(GuardianExtractionInvocation::Rejected(response))
                }
            }
        }
    }
    .and_then(|result| result.extract(operation))
}

#[allow(clippy::large_enum_variant)]
enum GuardianExtractionInvocation {
    Claude(WireResponse<crate::wire::claude::generate_content::GenerateContentResponseBody>),
    Gemini(WireResponse<crate::wire::gemini::GenerateContentResponseBody>),
    Chat(WireResponse<crate::wire::openai::chat::response::GenerateContentResponseBody>),
    Responses(WireResponse<crate::wire::openai::responses::GenerateContentResponseBody>),
    Rejected(WireResponse<HttpBody>),
}

enum Extracted {
    Review {
        result: GuardianReviewResult,
        native: GuardianNativeResponse,
    },
    Classify {
        result: GuardianClassifyResult,
        native: GuardianNativeResponse,
    },
    Rejected(WireResponse<HttpBody>),
    Invalid {
        error: TransformError,
        native: GuardianNativeResponse,
    },
}

impl GuardianExtractionInvocation {
    fn extract(self, operation: GuardianOperation) -> Result<Extracted, TransformError> {
        match self {
            Self::Claude(response) => map_response(
                response,
                operation,
                guardian::extract_claude,
                GuardianNativeResponse::Claude,
            ),
            Self::Gemini(response) => map_response(
                response,
                operation,
                guardian::extract_gemini,
                GuardianNativeResponse::Gemini,
            ),
            Self::Chat(response) => map_response(
                response,
                operation,
                guardian::extract_chat,
                GuardianNativeResponse::Chat,
            ),
            Self::Responses(response) => map_response(
                response,
                operation,
                guardian::extract_responses,
                GuardianNativeResponse::Responses,
            ),
            Self::Rejected(response) => Ok(Extracted::Rejected(response)),
        }
    }
}

fn map_response<T, F>(
    response: WireResponse<T>,
    operation: GuardianOperation,
    extract: F,
    native: impl FnOnce(WireResponse<T>) -> GuardianNativeResponse,
) -> Result<Extracted, TransformError>
where
    F: FnOnce(T, GuardianOperation) -> Result<GuardianExtraction, TransformError>,
    T: Clone,
{
    let WireResponse {
        status,
        headers,
        body,
    } = response;
    let parsed = extract(body.clone(), operation);
    let native = native(WireResponse {
        status,
        headers,
        body,
    });
    match parsed {
        Err(error) => Ok(Extracted::Invalid { error, native }),
        Ok(GuardianExtraction::Review(value)) => Ok(Extracted::Review {
            result: value,
            native,
        }),
        Ok(GuardianExtraction::Classify(value)) => Ok(Extracted::Classify {
            result: value,
            native,
        }),
    }
}

fn map_review(value: Extracted) -> GuardianReviewInvocation {
    match value {
        Extracted::Review { result, native } => {
            GuardianReviewInvocation::Success { result, native }
        }
        Extracted::Rejected(value) => GuardianReviewInvocation::Rejected(value),
        Extracted::Invalid { error, native } => GuardianReviewInvocation::Invalid { error, native },
        Extracted::Classify { .. } => unreachable!("review extraction returned classify result"),
    }
}
fn map_classify(value: Extracted) -> GuardianClassifyInvocation {
    match value {
        Extracted::Classify { result, native } => {
            GuardianClassifyInvocation::Success { result, native }
        }
        Extracted::Rejected(value) => GuardianClassifyInvocation::Rejected(value),
        Extracted::Invalid { error, native } => {
            GuardianClassifyInvocation::Invalid { error, native }
        }
        Extracted::Review { .. } => unreachable!("classify extraction returned review result"),
    }
}

mod stream;
pub use stream::{GuardianStreamContext, GuardianStreamInvocation, GuardianStreamLimits};

mod invoke_sse;
pub use invoke_sse::{classify_sse, review_sse};

mod resources;
pub use resources::{GuardianResourceLimits, prepare_with_resources};
