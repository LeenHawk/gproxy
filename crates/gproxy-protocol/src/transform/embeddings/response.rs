use super::common;
use crate::transform::{Converted, Report, TransformError};
use crate::wire::{gemini, openai};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiResponseSupplement {
    pub model: String,
    pub usage: Option<OpenAiUsageFacts>,
    pub encoding_format: openai::embeddings::EmbeddingEncodingFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenAiUsageFacts {
    pub prompt_tokens: i64,
    pub total_tokens: i64,
}

/// Request-derived facts kept outside the vendor wire. Response model defaults
/// to the actual selected upstream; an alias policy belongs to the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingResponseContext {
    pub requested_model: String,
    pub upstream_model: String,
    pub encoding_format: openai::embeddings::EmbeddingEncodingFormat,
    pub expected_count: usize,
    pub expected_dimensions: Option<i64>,
}

impl EmbeddingResponseContext {
    pub fn finish_single(
        &self,
        input: gemini::embeddings::EmbedContentResponseBody,
        usage: Option<OpenAiUsageFacts>,
    ) -> Result<Converted<openai::embeddings::CreateEmbeddingResponseBody>, TransformError> {
        let _embeddings: Vec<_> = input.embedding.iter().collect();

        gemini_single_response_to_openai(input, &self.supplement(usage))
    }

    pub fn finish_batch(
        &self,
        input: gemini::embeddings::BatchEmbedContentsResponseBody,
        usage: Option<OpenAiUsageFacts>,
    ) -> Result<Converted<openai::embeddings::CreateEmbeddingResponseBody>, TransformError> {
        let _embeddings: Vec<_> = input.embeddings.iter().flatten().collect();

        gemini_batch_response_to_openai(input, &self.supplement(usage))
    }

    fn supplement(&self, usage: Option<OpenAiUsageFacts>) -> OpenAiResponseSupplement {
        OpenAiResponseSupplement {
            model: self.upstream_model.clone(),
            encoding_format: self.encoding_format,
            usage,
        }
    }
}

pub fn openai_single_response_to_gemini(
    input: openai::embeddings::CreateEmbeddingResponseBody,
) -> Result<Converted<gemini::embeddings::EmbedContentResponseBody>, TransformError> {
    let embedding = input
        .data
        .into_iter()
        .next()
        .map(gemini_embedding)
        .map(crate::transform::optional)
        .transpose()?
        .flatten();
    let report = openai_response_report(&input.usage);
    Ok(common::converted(
        gemini::embeddings::EmbedContentResponseBody {
            embedding,
            usage_metadata: Some(gemini_usage(input.usage)?),
            rest: Default::default(),
        },
        report,
    ))
}

pub fn openai_batch_response_to_gemini(
    input: openai::embeddings::CreateEmbeddingResponseBody,
) -> Result<Converted<gemini::embeddings::BatchEmbedContentsResponseBody>, TransformError> {
    let mut embeddings = Vec::with_capacity(input.data.len());
    for (expected, item) in input.data.into_iter().enumerate() {
        let index = i64::try_from(expected).map_err(|_| {
            TransformError::invalid_result(
                "embedding.index",
                "embedding index exceeds integer range",
            )
        })?;
        if item.index != index {
            return Err(TransformError::invalid_result(
                "embedding.index",
                "OpenAI indexes are not in sample order",
            ));
        }
        embeddings.push(gemini_embedding(item)?);
    }
    if embeddings.is_empty() {
        return Err(TransformError::invalid_result(
            "embedding.data",
            "batch response has no embeddings",
        ));
    }
    let report = openai_response_report(&input.usage);
    Ok(common::converted(
        gemini::embeddings::BatchEmbedContentsResponseBody {
            embeddings: Some(embeddings),
            usage_metadata: Some(gemini_usage(input.usage)?),
            rest: Default::default(),
        },
        report,
    ))
}

pub fn gemini_single_response_to_openai(
    input: gemini::embeddings::EmbedContentResponseBody,
    supplement: &OpenAiResponseSupplement,
) -> Result<Converted<openai::embeddings::CreateEmbeddingResponseBody>, TransformError> {
    let embedding = input.embedding.ok_or_else(|| {
        TransformError::invalid_result("embedding", "Gemini response has no embedding")
    })?;
    let item = openai_embedding(0, embedding, supplement.encoding_format)?;
    let mut report = gemini_response_report(input.usage_metadata.as_ref());
    let usage = openai_usage(input.usage_metadata, supplement.usage)?;
    if matches!(
        supplement.encoding_format,
        openai::embeddings::EmbeddingEncodingFormat::Base64
    ) {
        report.changed(
            "embedding.encoding_format",
            "Gemini JSON values are encoded as IEEE754 binary32 little-endian bytes",
        );
    }
    Ok(common::converted(
        openai::embeddings::CreateEmbeddingResponseBody {
            data: vec![item],
            model: nonempty_model(&supplement.model)?,
            object_: openai::embeddings::EmbeddingListObject::List,
            usage,
            rest: Default::default(),
        },
        report,
    ))
}

pub fn gemini_batch_response_to_openai(
    input: gemini::embeddings::BatchEmbedContentsResponseBody,
    supplement: &OpenAiResponseSupplement,
) -> Result<Converted<openai::embeddings::CreateEmbeddingResponseBody>, TransformError> {
    let embeddings = input.embeddings.ok_or_else(|| {
        TransformError::invalid_result("embeddings", "Gemini batch response has no embeddings")
    })?;
    if embeddings.is_empty() {
        return Err(TransformError::invalid_result(
            "embeddings",
            "Gemini batch response has no embeddings",
        ));
    }
    let data = embeddings
        .into_iter()
        .enumerate()
        .map(|(index, embedding)| {
            let index = i64::try_from(index).map_err(|_| {
                TransformError::invalid_result(
                    "embedding.index",
                    "embedding index exceeds integer range",
                )
            })?;
            openai_embedding(index, embedding, supplement.encoding_format)
        })
        .filter_map(|value| crate::transform::optional(value).transpose())
        .collect::<Result<Vec<_>, _>>()?;
    let mut report = gemini_response_report(input.usage_metadata.as_ref());
    let usage = openai_usage(input.usage_metadata, supplement.usage)?;
    if matches!(
        supplement.encoding_format,
        openai::embeddings::EmbeddingEncodingFormat::Base64
    ) {
        report.changed(
            "embedding.encoding_format",
            "Gemini JSON values are encoded as IEEE754 binary32 little-endian bytes",
        );
    }
    Ok(common::converted(
        openai::embeddings::CreateEmbeddingResponseBody {
            data,
            model: nonempty_model(&supplement.model)?,
            object_: openai::embeddings::EmbeddingListObject::List,
            usage,
            rest: Default::default(),
        },
        report,
    ))
}

fn gemini_embedding(
    input: openai::embeddings::OpenAiEmbedding,
) -> Result<gemini::embeddings::ContentEmbedding, TransformError> {
    let values = common::vector_values(input.embedding)?;

    let dimension = i64::try_from(values.len()).map_err(|_| {
        TransformError::invalid_result("embedding.dimension", "vector length exceeds integer range")
    })?;
    Ok(gemini::embeddings::ContentEmbedding {
        values: Some(values),
        shape: Some(vec![dimension]),
        rest: Default::default(),
    })
}

fn openai_embedding(
    index: i64,
    input: gemini::embeddings::ContentEmbedding,
    format: openai::embeddings::EmbeddingEncodingFormat,
) -> Result<openai::embeddings::OpenAiEmbedding, TransformError> {
    let values = input
        .values
        .ok_or_else(|| TransformError::invalid_result("embedding.values", "values are absent"))?;

    let embedding = match format {
        openai::embeddings::EmbeddingEncodingFormat::Float => {
            openai::embeddings::EmbeddingVector::Floats(values)
        }
        openai::embeddings::EmbeddingEncodingFormat::Base64 => {
            openai::embeddings::EmbeddingVector::Base64(common::encode_base64(&values)?)
        }
    };
    Ok(openai::embeddings::OpenAiEmbedding {
        embedding,
        index,
        object_: openai::embeddings::EmbeddingObject::Embedding,
        rest: Default::default(),
    })
}

fn gemini_usage(
    usage: openai::embeddings::EmbeddingUsage,
) -> Result<gemini::embeddings::EmbeddingUsageMetadata, TransformError> {
    if usage.prompt_tokens < 0 || usage.total_tokens < usage.prompt_tokens {
        return Err(TransformError::invalid_result(
            "usage",
            "usage counts must be non-negative and total must cover prompt tokens",
        ));
    }
    Ok(gemini::embeddings::EmbeddingUsageMetadata {
        prompt_token_count: Some(usage.prompt_tokens),
        prompt_token_details: None,
        rest: Default::default(),
    })
}

fn openai_usage(
    source: Option<gemini::embeddings::EmbeddingUsageMetadata>,
    supplement: Option<OpenAiUsageFacts>,
) -> Result<openai::embeddings::EmbeddingUsage, TransformError> {
    let source = source.and_then(|usage| usage.prompt_token_count);
    let facts = match (source, supplement) {
        (Some(source), Some(facts)) => {
            if source != facts.prompt_tokens {
                return Err(TransformError::new(
                    crate::transform::TransformErrorKind::Conflict,
                    "usage.prompt_tokens",
                    "Gemini usage and supplied usage disagree",
                ));
            }
            OpenAiUsageFacts {
                prompt_tokens: source,
                total_tokens: facts.total_tokens,
            }
        }
        (Some(source), None) => OpenAiUsageFacts {
            prompt_tokens: source,
            total_tokens: source,
        },
        (None, Some(facts)) => facts,
        (None, None) => {
            return Err(TransformError::missing_metadata(
                "usage.prompt_tokens and usage.total_tokens",
            ));
        }
    };
    if facts.prompt_tokens < 0 || facts.total_tokens < facts.prompt_tokens {
        return Err(TransformError::invalid_result(
            "usage",
            "usage counts must be non-negative and total must cover prompt tokens",
        ));
    }
    Ok(openai::embeddings::EmbeddingUsage {
        prompt_tokens: facts.prompt_tokens,
        total_tokens: facts.total_tokens,
        rest: Default::default(),
    })
}

fn nonempty_model(value: &str) -> Result<String, TransformError> {
    if value.is_empty() {
        Err(TransformError::missing_metadata("target_model"))
    } else {
        Ok(value.to_owned())
    }
}

fn openai_response_report(usage: &openai::embeddings::EmbeddingUsage) -> Report {
    let mut report = Report::default();
    report.omitted("model", "Gemini embedding responses have no model field");
    if usage.total_tokens != usage.prompt_tokens {
        report.omitted(
            "usage.total_tokens",
            "Gemini embedding usage has no distinct total-token field",
        );
    }
    report
}

fn gemini_response_report(usage: Option<&gemini::embeddings::EmbeddingUsageMetadata>) -> Report {
    let mut report = Report::default();
    if usage.is_some_and(|usage| usage.prompt_token_details.is_some()) {
        report.omitted(
            "usage.prompt_token_details",
            "OpenAI embedding usage has no per-modality details",
        );
    }
    report
}
