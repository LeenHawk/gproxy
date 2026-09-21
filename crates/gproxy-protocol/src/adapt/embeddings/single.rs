use super::{EmbeddingBatchError, EmbeddingBatchResult, EmbeddingFailure, at, packing};
use crate::{
    WireRequest,
    adapt::{JsonInvocation, invoke_json},
    capability::Upstream,
    codec::{self, CodecLimits},
    transform::{
        Converted, TransformError,
        embeddings::{self, OpenAiUsageFacts},
    },
    wire::{gemini::embeddings as g, openai::embeddings as o},
};

/// One OpenAI input through Gemini embedContent, retaining requested encoding
/// and validating the returned dimension and factual usage.
pub async fn openai_to_gemini_single<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    template: WireRequest<()>,
    input: o::CreateEmbeddingRequestBody,
    target_model: &str,
    limits: CodecLimits,
    usage: Option<OpenAiUsageFacts>,
) -> Result<EmbeddingBatchResult<o::CreateEmbeddingResponseBody>, EmbeddingBatchError> {
    if usage.is_some_and(|v| v.prompt_tokens < 0 || v.total_tokens < v.prompt_tokens) {
        return Err(TransformError::shape(
            "embedding.usage",
            "usage must be nonnegative and total must cover prompt tokens",
        )
        .into());
    }
    let prepared = embeddings::openai_to_gemini_single(input, target_model)?;
    let WireRequest {
        method,
        path,
        query,
        headers,
        body: (),
    } = template;
    let response: JsonInvocation<g::EmbedContentResponseBody> = invoke_json(
        upstream,
        target,
        WireRequest {
            method,
            path,
            query,
            headers,
            body: prepared.value.body,
        },
        limits,
    )
    .await?;
    let response = match response {
        JsonInvocation::Success(response) => response.body,
        JsonInvocation::Rejected(response) => {
            return Err(EmbeddingBatchError {
                completed_calls: 0,
                completed_items: 0,
                failure: EmbeddingFailure::Rejected(Box::new(response)),
            });
        }
    };
    let mut output = prepared
        .value
        .response
        .finish_single(response, usage)
        .map_err(|error| at(error, 1, 0))?;
    let mut report = prepared.report;
    report.diagnostics.append(&mut output.report.diagnostics);
    output.report = report;
    codec::encode_json(&output.value, limits)
        .map_err(|error| at(packing::encoding(error), 1, 0))?;
    Ok(EmbeddingBatchResult {
        output,
        completed_calls: 1,
    })
}

/// One Gemini input through OpenAI embeddings. The selected target request
/// carries its explicit model and declared dimensionality.
pub async fn gemini_single_to_openai<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    template: WireRequest<()>,
    input: g::EmbedContentRequestBody,
    target_model: &str,
    limits: CodecLimits,
) -> Result<EmbeddingBatchResult<g::EmbedContentResponseBody>, EmbeddingBatchError> {
    let prepared = embeddings::gemini_single_to_openai(input, target_model)?;
    let dimensions = prepared.value.dimensions;
    let WireRequest {
        method,
        path,
        query,
        headers,
        body: (),
    } = template;
    let response: JsonInvocation<o::CreateEmbeddingResponseBody> = invoke_json(
        upstream,
        target,
        WireRequest {
            method,
            path,
            query,
            headers,
            body: prepared.value,
        },
        limits,
    )
    .await?;
    let response = match response {
        JsonInvocation::Success(response) => response.body,
        JsonInvocation::Rejected(response) => {
            return Err(EmbeddingBatchError {
                completed_calls: 0,
                completed_items: 0,
                failure: EmbeddingFailure::Rejected(Box::new(response)),
            });
        }
    };
    let converted =
        embeddings::openai_single_response_to_gemini(response).map_err(|error| at(error, 1, 0))?;
    if let Some(expected) = dimensions {
        let actual = converted
            .value
            .embedding
            .as_ref()
            .and_then(|v| v.values.as_ref())
            .and_then(|v| i64::try_from(v.len()).ok());
        if actual != Some(expected) {
            return Err(at(
                TransformError::invalid_result(
                    "embedding.dimensions",
                    "response dimensions differ from request",
                ),
                1,
                0,
            ));
        }
    }
    let mut report = prepared.report;
    report.diagnostics.extend(converted.report.diagnostics);
    codec::encode_json(&converted.value, limits)
        .map_err(|error| at(packing::encoding(error), 1, 0))?;
    Ok(EmbeddingBatchResult {
        output: Converted {
            value: converted.value,
            report,
        },
        completed_calls: 1,
    })
}
