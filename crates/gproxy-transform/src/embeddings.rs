use bytes::Bytes;
use gproxy_protocol::openai::embeddings as openai_embeddings;
use gproxy_protocol::{gemini, openai};

use crate::TransformError;

pub(crate) fn openai_to_gemini_single(body: Bytes, model: &str) -> Result<Bytes, TransformError> {
    let input: openai_embeddings::CreateEmbeddingRequest = serde_json::from_slice(&body)?;
    let mut requests = gemini_requests(input, model)?;
    if requests.len() != 1 {
        return Err(TransformError::shape(
            "OpenAI embeddings",
            "multiple inputs require Gemini batchEmbedContents",
        ));
    }
    encode(&requests.remove(0))
}

pub(crate) fn openai_to_gemini_batch(body: Bytes, model: &str) -> Result<Bytes, TransformError> {
    let input: openai_embeddings::CreateEmbeddingRequest = serde_json::from_slice(&body)?;
    encode(&gemini::BatchEmbedContentsRequest {
        requests: gemini_requests(input, model)?,
        rest: Default::default(),
    })
}

pub(crate) fn gemini_single_to_openai(body: Bytes, model: &str) -> Result<Bytes, TransformError> {
    let input: gemini::EmbedContentRequest = serde_json::from_slice(&body)?;
    encode(&openai_request(vec![input], model)?)
}

pub(crate) fn gemini_batch_to_openai(body: Bytes, model: &str) -> Result<Bytes, TransformError> {
    let input: gemini::BatchEmbedContentsRequest = serde_json::from_slice(&body)?;
    encode(&openai_request(input.requests, model)?)
}

pub(crate) fn gemini_single_response_to_openai(body: Bytes) -> Result<Bytes, TransformError> {
    let input: gemini::EmbedContentResponse = serde_json::from_slice(&body)?;
    let data = input
        .embedding
        .into_iter()
        .enumerate()
        .map(openai_embedding)
        .collect();
    encode(&openai_embeddings::CreateEmbeddingResponse {
        data,
        model: "unknown".into(),
        object: openai::ListObjectType::List,
        usage: openai_usage(input.usage_metadata),
        rest: Default::default(),
    })
}

pub(crate) fn gemini_batch_response_to_openai(body: Bytes) -> Result<Bytes, TransformError> {
    let input: gemini::BatchEmbedContentsResponse = serde_json::from_slice(&body)?;
    let data = input
        .embeddings
        .into_iter()
        .enumerate()
        .map(openai_embedding)
        .collect();
    encode(&openai_embeddings::CreateEmbeddingResponse {
        data,
        model: "unknown".into(),
        object: openai::ListObjectType::List,
        usage: openai_usage(input.usage_metadata),
        rest: Default::default(),
    })
}

pub(crate) fn openai_response_to_gemini_single(body: Bytes) -> Result<Bytes, TransformError> {
    let input: openai_embeddings::CreateEmbeddingResponse = serde_json::from_slice(&body)?;
    encode(&gemini::EmbedContentResponse {
        embedding: input
            .data
            .into_iter()
            .next()
            .map(gemini_embedding)
            .transpose()?,
        usage_metadata: Some(gemini_usage(input.usage)),
        rest: Default::default(),
    })
}

pub(crate) fn openai_response_to_gemini_batch(body: Bytes) -> Result<Bytes, TransformError> {
    let input: openai_embeddings::CreateEmbeddingResponse = serde_json::from_slice(&body)?;
    encode(&gemini::BatchEmbedContentsResponse {
        embeddings: input
            .data
            .into_iter()
            .map(gemini_embedding)
            .collect::<Result<_, _>>()?,
        usage_metadata: Some(gemini_usage(input.usage)),
        rest: Default::default(),
    })
}

fn gemini_requests(
    input: openai_embeddings::CreateEmbeddingRequest,
    model: &str,
) -> Result<Vec<gemini::EmbedContentRequest>, TransformError> {
    let texts = match input.input {
        openai_embeddings::EmbeddingInput::Text(text) => vec![text],
        openai_embeddings::EmbeddingInput::TextList(texts) => texts,
        openai_embeddings::EmbeddingInput::TokenList(_)
        | openai_embeddings::EmbeddingInput::TokenLists(_)
        | openai_embeddings::EmbeddingInput::Raw(_) => {
            return Err(TransformError::unsupported(
                "OpenAI embeddings input",
                "token ids have no Gemini wire equivalent",
            ));
        }
    };
    let dimensions = input
        .dimensions
        .map(|value| i32::try_from(value).unwrap_or(i32::MAX));
    Ok(texts
        .into_iter()
        .map(|text| gemini::EmbedContentRequest {
            model: Some(model.to_owned()),
            content: text_content(text),
            task_type: None,
            title: None,
            output_dimensionality: dimensions,
            embed_content_config: dimensions.map(|output_dimensionality| {
                gemini::EmbedContentConfig {
                    output_dimensionality: Some(output_dimensionality),
                    ..Default::default()
                }
            }),
            rest: Default::default(),
        })
        .collect())
}

fn openai_request(
    requests: Vec<gemini::EmbedContentRequest>,
    model: &str,
) -> Result<openai_embeddings::CreateEmbeddingRequest, TransformError> {
    let mut texts = Vec::with_capacity(requests.len());
    let mut dimensions = None;
    for request in requests {
        texts.push(content_text(request.content)?);
        let next = request
            .embed_content_config
            .and_then(|config| config.output_dimensionality)
            .or(request.output_dimensionality)
            .and_then(|value| u32::try_from(value).ok());
        if dimensions.is_some() && next.is_some() && dimensions != next {
            return Err(TransformError::shape(
                "Gemini embeddings",
                "batch requests use different output dimensionality",
            ));
        }
        dimensions = dimensions.or(next);
    }
    let input = if texts.len() == 1 {
        openai_embeddings::EmbeddingInput::Text(texts.remove(0))
    } else {
        openai_embeddings::EmbeddingInput::TextList(texts)
    };
    Ok(openai_embeddings::CreateEmbeddingRequest {
        input,
        model: model.into(),
        dimensions,
        encoding_format: Some(openai_embeddings::EmbeddingEncodingFormat::Known(
            openai_embeddings::KnownEmbeddingEncodingFormat::Float,
        )),
        user: None,
        rest: Default::default(),
    })
}

fn text_content(text: String) -> gemini::Content {
    gemini::Content {
        parts: vec![gemini::Part {
            data: Some(gemini::PartData::Text {
                text,
                rest: Default::default(),
            }),
            ..Default::default()
        }],
        ..Default::default()
    }
}

fn content_text(content: gemini::Content) -> Result<String, TransformError> {
    let mut text = String::new();
    for part in content.parts {
        match part.data {
            Some(gemini::PartData::Text { text: value, .. }) => text.push_str(&value),
            None => {}
            Some(_) => {
                return Err(TransformError::unsupported(
                    "Gemini embedding content",
                    "non-text part",
                ));
            }
        }
    }
    Ok(text)
}

fn openai_embedding(
    (index, input): (usize, gemini::ContentEmbedding),
) -> openai_embeddings::Embedding {
    openai_embeddings::Embedding {
        embedding: openai_embeddings::EmbeddingVector::Float(
            input.values.into_iter().map(f64::from).collect(),
        ),
        index: u32::try_from(index).unwrap_or(u32::MAX),
        object: openai::EmbeddingObjectType::Embedding,
        rest: Default::default(),
    }
}

fn gemini_embedding(
    input: openai_embeddings::Embedding,
) -> Result<gemini::ContentEmbedding, TransformError> {
    let openai_embeddings::EmbeddingVector::Float(values) = input.embedding else {
        return Err(TransformError::unsupported(
            "OpenAI embedding vector",
            "non-float vector",
        ));
    };
    Ok(gemini::ContentEmbedding {
        values: values.into_iter().map(|value| value as f32).collect(),
        shape: Vec::new(),
        rest: Default::default(),
    })
}

fn openai_usage(
    input: Option<gemini::EmbeddingUsageMetadata>,
) -> openai_embeddings::EmbeddingUsage {
    let tokens = input
        .as_ref()
        .and_then(|usage| usage.prompt_token_count)
        .and_then(|value| u64::try_from(value).ok())
        .unwrap_or_default();
    openai_embeddings::EmbeddingUsage {
        prompt_tokens: tokens,
        total_tokens: tokens,
        rest: Default::default(),
    }
}

fn gemini_usage(input: openai_embeddings::EmbeddingUsage) -> gemini::EmbeddingUsageMetadata {
    gemini::EmbeddingUsageMetadata {
        prompt_token_count: Some(i32::try_from(input.prompt_tokens).unwrap_or(i32::MAX)),
        prompt_token_details: Vec::new(),
        rest: Default::default(),
    }
}

fn encode(value: &impl serde::Serialize) -> Result<Bytes, TransformError> {
    Ok(Bytes::from(serde_json::to_vec(value)?))
}
