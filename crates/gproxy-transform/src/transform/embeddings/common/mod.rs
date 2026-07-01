use crate::protocol::{gemini, openai};

pub(in crate::transform::embeddings) const DEFAULT_OPENAI_EMBEDDING_MODEL: &str = "unknown";

pub(in crate::transform::embeddings) struct ConvertedGeminiRequest {
    pub text: String,
    pub model: Option<String>,
    pub dimensions: Option<u32>,
}

pub(in crate::transform::embeddings) fn openai_to_gemini_requests(
    input: openai::EmbeddingRequest,
) -> Vec<gemini::EmbedContentRequest> {
    let model = model_name(&input.model);
    let embed_content_config = input
        .dimensions
        .map(|dimensions| gemini::EmbedContentConfig {
            output_dimensionality: Some(u32_to_i32(dimensions)),
            ..Default::default()
        });

    input
        .input
        .into_strings()
        .into_iter()
        .map(|text| gemini::EmbedContentRequest {
            model: Some(model.clone()),
            content: Some(text_content(text)),
            task_type: None,
            title: None,
            output_dimensionality: None,
            embed_content_config: embed_content_config.clone(),
            extra: Default::default(),
        })
        .collect()
}

pub(in crate::transform::embeddings) fn openai_to_gemini_embedding(
    input: openai::Embedding,
) -> gemini::ContentEmbedding {
    gemini::ContentEmbedding {
        values: input
            .embedding
            .into_iter()
            .map(|value| value as f32)
            .collect(),
        shape: Vec::new(),
        extra: Default::default(),
    }
}

pub(in crate::transform::embeddings) fn openai_to_gemini_usage(
    input: openai::EmbeddingUsage,
) -> gemini::EmbeddingUsageMetadata {
    gemini::EmbeddingUsageMetadata {
        prompt_token_count: Some(u32_to_i32(input.prompt_tokens)),
        total_token_count: Some(u32_to_i32(input.total_tokens)),
        input_token_count: Some(u32_to_i32(input.prompt_tokens)),
        prompt_token_details: Vec::new(),
        prompt_tokens_details: Vec::new(),
        extra: Default::default(),
    }
}

pub(in crate::transform::embeddings) fn gemini_request_parts(
    input: gemini::EmbedContentRequest,
) -> ConvertedGeminiRequest {
    let dimensions = input
        .embed_content_config
        .as_ref()
        .and_then(|config| config.output_dimensionality)
        .or(input.output_dimensionality)
        .map(i32_to_u32);

    ConvertedGeminiRequest {
        text: content_text(input.content),
        model: input.model,
        dimensions,
    }
}

pub(in crate::transform::embeddings) fn strings_to_openai_input(
    values: Vec<String>,
) -> openai::EmbeddingInput {
    let mut values = values;
    if values.len() == 1 {
        openai::EmbeddingInput::Text(values.remove(0))
    } else {
        openai::EmbeddingInput::TextList(values)
    }
}

pub(in crate::transform::embeddings) fn merge_model(
    target: &mut Option<String>,
    next: Option<String>,
) {
    let Some(next) = next else {
        return;
    };
    if target.is_none() {
        *target = Some(next);
    }
}

pub(in crate::transform::embeddings) fn merge_dimensions(
    target: &mut Option<u32>,
    next: Option<u32>,
) {
    let Some(next) = next else {
        return;
    };
    if target.is_none() {
        *target = Some(next);
    }
}

pub(in crate::transform::embeddings) fn gemini_to_openai_embedding(
    input: gemini::ContentEmbedding,
    index: usize,
) -> openai::Embedding {
    openai::Embedding {
        embedding: input.values.into_iter().map(f64::from).collect(),
        index: u32::try_from(index).unwrap_or(u32::MAX),
        object: openai::EmbeddingObjectType::Embedding,
        extra: Default::default(),
    }
}

pub(in crate::transform::embeddings) fn gemini_to_openai_usage(
    input: Option<gemini::EmbeddingUsageMetadata>,
) -> openai::EmbeddingUsage {
    let Some(input) = input else {
        return openai::EmbeddingUsage {
            prompt_tokens: 0,
            total_tokens: 0,
            extra: Default::default(),
        };
    };

    let prompt_tokens = input.prompt_token_count.or(input.input_token_count);
    let total_tokens = input.total_token_count.or(prompt_tokens);

    openai::EmbeddingUsage {
        prompt_tokens: prompt_tokens.map(i32_to_u32).unwrap_or_default(),
        total_tokens: total_tokens.map(i32_to_u32).unwrap_or_default(),
        extra: Default::default(),
    }
}

fn i32_to_u32(value: i32) -> u32 {
    u32::try_from(value).unwrap_or_default()
}

fn u32_to_i32(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

fn model_name(model: &openai::OpenAiModelId) -> String {
    let Ok(value) = serde_json::to_value(model) else {
        return String::new();
    };
    value.as_str().map(str::to_owned).unwrap_or_default()
}

fn text_content(text: String) -> gemini::Content {
    gemini::Content {
        parts: vec![gemini::Part {
            thought: None,
            thought_signature: None,
            part_metadata: None,
            media_resolution: None,
            data: Some(gemini::PartData::Text { text }),
            metadata: None,
            extra: Default::default(),
        }],
        role: None,
        extra: Default::default(),
    }
}

fn content_text(input: Option<gemini::Content>) -> String {
    let Some(content) = input else {
        return String::new();
    };
    let mut text = String::new();

    for part in content.parts {
        if let Some(gemini::PartData::Text { text: value }) = part.data {
            text.push_str(&value);
        }
    }

    text
}

trait EmbeddingInputExt {
    fn into_strings(self) -> Vec<String>;
}

impl EmbeddingInputExt for openai::EmbeddingInput {
    fn into_strings(self) -> Vec<String> {
        match self {
            Self::Text(text) => vec![text],
            Self::TextList(values) => values,
            Self::TokenList(_) => vec![String::new()],
            Self::TokenLists(values) => values.into_iter().map(|_| String::new()).collect(),
        }
    }
}
