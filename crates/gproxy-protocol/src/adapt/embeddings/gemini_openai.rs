use super::{
    EmbeddingBatchError, EmbeddingBatchOptions, EmbeddingBatchResult, EmbeddingFailure, at,
    openai_packing, packing,
};
use crate::{
    WireRequest,
    adapt::{JsonInvocation, invoke_json},
    capability::Upstream,
    codec,
    transform::{Converted, Report, TransformError, embeddings},
    wire::{DeclaredFields, gemini::embeddings as g, openai::embeddings as o},
};

/// Execute a Gemini embedding batch through the selected OpenAI model. Varying
/// output dimensions are split into separate calls and returned in input order.
pub async fn gemini_batch_to_openai<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    template: WireRequest<()>,
    input: g::BatchEmbedContentsRequestBody,
    target_model: &str,
    options: EmbeddingBatchOptions,
) -> Result<EmbeddingBatchResult<g::BatchEmbedContentsResponseBody>, EmbeddingBatchError> {
    let input = input.into_declared();
    if input.requests.is_empty() {
        return Err(
            TransformError::shape("embedding.requests", "batch must contain a request").into(),
        );
    }
    let mut prepared = Vec::with_capacity(input.requests.len());
    let mut report = Report::default();
    for request in input.requests {
        if !request.model.starts_with("models/") || request.model.len() == "models/".len() {
            return Err(TransformError::shape(
                "embedding.model",
                "source Gemini model must be a complete models/{name} resource",
            )
            .into());
        }
        let converted = embeddings::gemini_single_to_openai(
            g::EmbedContentRequestBody {
                content: request.content,
                task_type: request.task_type,
                title: request.title,
                output_dimensionality: request.output_dimensionality,
                embed_content_config: request.embed_content_config,
                rest: Default::default(),
            },
            target_model,
        )?;
        report.diagnostics.extend(converted.report.diagnostics);
        prepared.push(converted.value);
    }
    let batches = openai_packing::batches(prepared, &options, upstream.limits().write_bytes)?;
    let WireRequest {
        method,
        path,
        query,
        headers,
        body: (),
    } = template;
    let mut vectors = Vec::new();
    let mut prompt_tokens = 0_i64;
    let mut completed = 0;
    for batch in batches {
        let expected = match &batch.input {
            o::EmbeddingInput::Texts(values) => values.len(),
            _ => unreachable!("prepared batch"),
        };
        let dimensions = batch.dimensions;
        let request = WireRequest {
            method: method.clone(),
            path: path.clone(),
            query: query.clone(),
            headers: headers.clone(),
            body: batch,
        };
        let result: JsonInvocation<o::CreateEmbeddingResponseBody> =
            invoke_json(upstream, target, request, options.codec)
                .await
                .map_err(|error| at(error, completed, vectors.len()))?;
        let mut response = match result {
            JsonInvocation::Success(response) => {
                completed += 1;
                response.body
            }
            JsonInvocation::Rejected(response) => {
                return Err(EmbeddingBatchError {
                    completed_calls: completed,
                    completed_items: vectors.len(),
                    failure: EmbeddingFailure::Rejected(Box::new(response)),
                });
            }
        };
        if response.data.len() != expected {
            return Err(at(
                TransformError::invalid_result(
                    "embedding.count",
                    "response sample count differs from request",
                ),
                completed,
                vectors.len(),
            ));
        }
        // OpenAI indexes define sample order independently of returned array order.
        // The typed converter below rejects gaps, duplicates and invalid indexes.
        response.data.sort_unstable_by_key(|item| item.index);
        let converted = embeddings::openai_batch_response_to_gemini(response)
            .map_err(|error| at(error, completed, vectors.len()))?;
        let part = converted.value.into_declared();
        let values = part.embeddings.ok_or_else(|| {
            at(
                TransformError::invalid_result("embeddings", "missing batch vectors"),
                completed,
                vectors.len(),
            )
        })?;
        if let Some(expected_dimensions) = dimensions {
            for value in &values {
                if value
                    .values
                    .as_ref()
                    .and_then(|v| i64::try_from(v.len()).ok())
                    != Some(expected_dimensions)
                {
                    return Err(at(
                        TransformError::invalid_result(
                            "embedding.dimensions",
                            "response dimensions differ from request",
                        ),
                        completed,
                        vectors.len(),
                    ));
                }
            }
        }
        let count = part
            .usage_metadata
            .and_then(|v| v.prompt_token_count)
            .ok_or_else(|| {
                at(
                    TransformError::missing_metadata("usage.prompt_tokens"),
                    completed,
                    vectors.len(),
                )
            })?;
        prompt_tokens = prompt_tokens.checked_add(count).ok_or_else(|| {
            at(
                TransformError::invalid_result(
                    "usage.prompt_tokens",
                    "aggregate token count overflow",
                ),
                completed,
                vectors.len(),
            )
        })?;
        let before = vectors.len();
        vectors.extend(values);
        report.diagnostics.extend(converted.report.diagnostics);
        codec::encode_json(&vectors, options.codec)
            .map_err(|error| at(packing::encoding(error), completed, before))?;
    }
    let value = g::BatchEmbedContentsResponseBody::builder()
        .embeddings(vectors)
        .usage_metadata(
            g::EmbeddingUsageMetadata::builder()
                .prompt_token_count(prompt_tokens)
                .build(),
        )
        .build();
    codec::encode_json(&value, options.codec).map_err(|error| {
        at(
            packing::encoding(error),
            completed,
            value.embeddings.as_ref().map_or(0, Vec::len),
        )
    })?;
    Ok(EmbeddingBatchResult {
        output: Converted { value, report },
        completed_calls: completed,
    })
}
