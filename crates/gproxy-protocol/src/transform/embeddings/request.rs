use super::common;
use crate::transform::{Converted, Report, TransformError};
use crate::wire::{gemini, openai};

/// A converted Gemini request and the OpenAI response facts captured before
/// consuming its source request. Keep `response` until the upstream completes;
/// this preserves the requested encoding, model identities and expected shape.
#[derive(Debug, Clone, PartialEq)]
pub struct PreparedEmbedding<T> {
    pub body: T,
    pub response: super::response::EmbeddingResponseContext,
}

pub fn openai_to_gemini_single(
    input: openai::embeddings::CreateEmbeddingRequestBody,
    target_model: &str,
) -> Result<Converted<PreparedEmbedding<gemini::embeddings::EmbedContentRequestBody>>, TransformError>
{
    let target_model = common::model_resource(target_model)?;
    let response = response_options(&input, &target_model, 1)?;
    let text = single_openai_text(input.input)?;
    let dimensions = common::dimensions(input.dimensions)?;
    let mut report = Report::default();
    if input.encoding_format == Some(openai::embeddings::EmbeddingEncodingFormat::Base64) {
        report.changed(
            "encoding_format",
            "base64 response preference is retained in the response context while Gemini returns numeric values",
        );
    }
    if input.user.is_some() {
        report.omitted("user", "Gemini embedding requests have no user field");
    }
    Ok(common::converted(
        PreparedEmbedding {
            response,
            body: gemini::embeddings::EmbedContentRequestBody {
                content: common::text_content(text),
                task_type: None,
                title: None,
                output_dimensionality: dimensions,
                embed_content_config: None,
                rest: Default::default(),
            },
        },
        report,
    ))
}

pub fn openai_to_gemini_batch(
    input: openai::embeddings::CreateEmbeddingRequestBody,
    target_model: &str,
) -> Result<
    Converted<PreparedEmbedding<gemini::embeddings::BatchEmbedContentsRequestBody>>,
    TransformError,
> {
    let target_model = common::model_resource(target_model)?;
    let count = match &input.input {
        openai::embeddings::EmbeddingInput::Texts(texts) => texts.len(),
        _ => 1,
    };
    let response = response_options(&input, &target_model, count)?;
    let texts = openai_texts(input.input)?;
    let dimensions = common::dimensions(input.dimensions)?;
    let mut report = Report::default();
    if input.encoding_format == Some(openai::embeddings::EmbeddingEncodingFormat::Base64) {
        report.changed(
            "encoding_format",
            "base64 response preference is retained in the response context while Gemini returns numeric values",
        );
    }
    if input.user.is_some() {
        report.omitted("user", "Gemini embedding requests have no user field");
    }
    let requests = texts
        .into_iter()
        .map(|text| gemini::embeddings::BatchEmbedContentRequest {
            model: target_model.clone(),
            content: common::text_content(text),
            task_type: None,
            title: None,
            output_dimensionality: dimensions,
            embed_content_config: None,
            rest: Default::default(),
        })
        .collect();
    Ok(common::converted(
        PreparedEmbedding {
            response,
            body: gemini::embeddings::BatchEmbedContentsRequestBody {
                requests,
                rest: Default::default(),
            },
        },
        report,
    ))
}

pub fn gemini_single_to_openai(
    input: gemini::embeddings::EmbedContentRequestBody,
    target_model: &str,
) -> Result<Converted<openai::embeddings::CreateEmbeddingRequestBody>, TransformError> {
    let target_model = common::target_model_name(target_model)?;
    let dimensions = common::gemini_dimensions(&input)?;
    let text = common::content_text(input.content)?;

    Ok(common::converted(
        openai::embeddings::CreateEmbeddingRequestBody {
            input: openai::embeddings::EmbeddingInput::Text(text),
            model: target_model,
            dimensions,
            encoding_format: Some(openai::embeddings::EmbeddingEncodingFormat::Float),
            user: None,
            rest: Default::default(),
        },
        Report::default(),
    ))
}

pub fn gemini_batch_to_openai(
    input: gemini::embeddings::BatchEmbedContentsRequestBody,
    target_model: &str,
) -> Result<Converted<openai::embeddings::CreateEmbeddingRequestBody>, TransformError> {
    let target_model = common::target_model_name(target_model)?;

    let mut texts = Vec::with_capacity(input.requests.len());
    let mut dimensions: Option<Option<i64>> = None;
    let mut source_model = None;
    for request in input.requests {
        let model = common::source_model_resource(&request.model)?;
        if let Some(previous) = &source_model
            && previous != &model
        {
            continue;
        }
        source_model = Some(model);

        let request_dimensions = common::dimensions(request.output_dimensionality)?;
        let config_dimensions = request
            .embed_content_config
            .as_ref()
            .and_then(|config| config.output_dimensionality);
        let request_dimensions = match (request_dimensions, common::dimensions(config_dimensions)?)
        {
            (Some(a), Some(b)) if a != b => {
                return Err(TransformError::new(
                    crate::transform::TransformErrorKind::Conflict,
                    "embedding.output_dimensionality",
                    "top-level and config dimensions disagree",
                ));
            }
            (Some(value), _) | (_, Some(value)) => Some(value),
            (None, None) => None,
        };
        if dimensions.is_some_and(|previous| previous != request_dimensions) {
            continue;
        }
        dimensions = Some(request_dimensions);
        texts.push(common::content_text(request.content)?);
    }
    Ok(common::converted(
        openai::embeddings::CreateEmbeddingRequestBody {
            input: openai::embeddings::EmbeddingInput::Texts(texts),
            model: target_model,
            dimensions: dimensions.flatten(),
            encoding_format: Some(openai::embeddings::EmbeddingEncodingFormat::Float),
            user: None,
            rest: Default::default(),
        },
        Report::default(),
    ))
}

fn single_openai_text(input: openai::embeddings::EmbeddingInput) -> Result<String, TransformError> {
    Ok(match input {
        openai::embeddings::EmbeddingInput::Text(text) => text,
        openai::embeddings::EmbeddingInput::Texts(texts) => {
            texts.into_iter().next().unwrap_or_default()
        }
        openai::embeddings::EmbeddingInput::Tokens(_)
        | openai::embeddings::EmbeddingInput::TokenArrays(_) => String::new(),
    })
}

fn openai_texts(input: openai::embeddings::EmbeddingInput) -> Result<Vec<String>, TransformError> {
    let texts = match input {
        openai::embeddings::EmbeddingInput::Text(text) => vec![text],
        openai::embeddings::EmbeddingInput::Texts(texts) => texts,
        openai::embeddings::EmbeddingInput::Tokens(_)
        | openai::embeddings::EmbeddingInput::TokenArrays(_) => {
            return Err(TransformError::unsupported(
                "embedding.input",
                "token ids require a source vocabulary and have no Gemini text equivalent",
            ));
        }
    };

    Ok(texts)
}

fn response_options(
    input: &openai::embeddings::CreateEmbeddingRequestBody,
    upstream_model: &str,
    expected_count: usize,
) -> Result<super::response::EmbeddingResponseContext, TransformError> {
    Ok(super::response::EmbeddingResponseContext {
        requested_model: common::target_model_name(&input.model)?,
        upstream_model: upstream_model.to_owned(),
        encoding_format: input
            .encoding_format
            .unwrap_or(openai::embeddings::EmbeddingEncodingFormat::Float),
        expected_count,
        expected_dimensions: common::dimensions(input.dimensions)?,
    })
}
