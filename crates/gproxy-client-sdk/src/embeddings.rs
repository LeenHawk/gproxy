//! Portable embeddings. The existing `Client::embeddings` native passthrough
//! remains available. Explicit embedding dialects do not enable chat renderers.
use crate::*;
use base64::Engine;
use protocol::{
    HttpBody, Operation, OperationKey, WireResponse,
    gemini::{self as g, embeddings as e},
    openai::embeddings as o,
};
use serde::{Deserialize, Serialize};
use std::num::{NonZeroU32, NonZeroUsize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum EmbeddingDialect {
    Openai,
    Gemini,
    #[serde(untagged)]
    Unknown(String),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct EmbeddingModel {
    pub id: String,
    pub dialect: EmbeddingDialect,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum EmbeddingInput {
    Text(String),
    Image(Image),
    Parts(Vec<EmbeddingPart>),
    #[serde(untagged)]
    Unknown(serde_json::Value),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum EmbeddingPart {
    Text(String),
    Image(Image),
    #[serde(untagged)]
    Unknown(serde_json::Value),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum EmbeddingPurpose {
    Query,
    Document,
    SemanticSimilarity,
    Classification,
    Clustering,
    QuestionAnswering,
    FactVerification,
    CodeQuery,
    #[serde(untagged)]
    Unknown(String),
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct EmbeddingOptions {
    pub openai: OpenaiEmbeddingOptions,
    pub gemini: GeminiEmbeddingOptions,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct OpenaiEmbeddingOptions {
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<i64>,
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>", optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding_format: Option<o::EmbeddingEncodingFormat>,
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, flatten)]
    pub rest: protocol::Rest,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GeminiEmbeddingOptions {
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>", optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<e::EmbedContentConfig>,
    #[serde(default, flatten)]
    pub rest: protocol::Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct EmbedRequest {
    pub model: EmbeddingModel,
    pub inputs: Vec<EmbeddingInput>,
    #[cfg_attr(feature = "ts", ts(as = "Option<u32>"))]
    pub dimensions: Option<NonZeroU32>,
    pub purpose: Option<EmbeddingPurpose>,
    /// Upper bound on native inputs per call. OpenAI packs at most 2048;
    /// Gemini uses one routed embedContent call per input.
    #[cfg_attr(feature = "ts", ts(as = "Option<usize>"))]
    pub batch_size: Option<NonZeroUsize>,
    #[serde(default)]
    pub provider_options: EmbeddingOptions,
}
impl EmbedRequest {
    pub fn new(model: EmbeddingModel, inputs: Vec<EmbeddingInput>) -> Self {
        Self {
            model,
            inputs,
            dimensions: None,
            purpose: None,
            batch_size: None,
            provider_options: EmbeddingOptions::default(),
        }
    }
    pub async fn resolve_assets<F, Fut>(&self, mut resolve: F) -> Result<PreparedEmbedRequest>
    where
        F: FnMut(ResolveAsset) -> Fut,
        Fut: std::future::Future<Output = Result<ResolvedAsset>>,
    {
        let mut request = self.clone();
        let target = match request.model.dialect {
            EmbeddingDialect::Openai => AssetDialect::OpenaiEmbedding,
            EmbeddingDialect::Gemini => AssetDialect::GeminiEmbedding,
            EmbeddingDialect::Unknown(_) => {
                return Err(Error::invalid("unknown embedding dialect"));
            }
        };
        let mut warnings = Vec::new();
        for (i, input) in request.inputs.iter_mut().enumerate() {
            match input {
                EmbeddingInput::Image(image) => {
                    crate::resolution::prepare_image(
                        image,
                        target,
                        AssetLocation::Embedding,
                        &format!("inputs[{i}]"),
                        &mut resolve,
                        &mut warnings,
                    )
                    .await?
                }
                EmbeddingInput::Parts(parts) => {
                    for (j, part) in parts.iter_mut().enumerate() {
                        if let EmbeddingPart::Image(image) = part {
                            crate::resolution::prepare_image(
                                image,
                                target,
                                AssetLocation::Embedding,
                                &format!("inputs[{i}].parts[{j}]"),
                                &mut resolve,
                                &mut warnings,
                            )
                            .await?;
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(PreparedEmbedRequest { request, warnings })
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct EmbedResult {
    pub vectors: Vec<Vec<f64>>,
    pub usage: Usage,
    pub warnings: Vec<Warning>,
}
/// Send-only embedding snapshot; resolved URLs cannot be serialized as history.
pub struct PreparedEmbedRequest {
    request: EmbedRequest,
    warnings: Vec<Warning>,
}
impl PreparedEmbedRequest {
    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }
}
struct Batch {
    vectors: Vec<Vec<f64>>,
    input_tokens: Option<u64>,
    warnings: Vec<Warning>,
}
impl<B: Backend> Client<B> {
    pub async fn embed_prepared(&self, prepared: PreparedEmbedRequest) -> Result<EmbedResult> {
        let mut result = self.embed(&prepared.request).await?;
        let mut warnings = prepared.warnings;
        warnings.extend(result.warnings);
        result.warnings = warnings;
        Ok(result)
    }
    pub async fn embed(&self, request: &EmbedRequest) -> Result<EmbedResult> {
        let mut warnings = Vec::new();
        let mut vectors = Vec::with_capacity(request.inputs.len());
        let mut tokens = Some(0u64);
        match &request.model.dialect {
            EmbeddingDialect::Openai => {
                let texts = request
                    .inputs
                    .iter()
                    .map(|input| match input {
                        EmbeddingInput::Text(text) => Ok(text.as_str()),
                        _ => Err(Error::invalid(
                            "OpenAI embeddings accept text inputs; use Gemini for images or multipart input",
                        )),
                    })
                    .collect::<Result<Vec<_>>>()?;
                if request.purpose.is_some() {
                    warnings.push(Warning::new(
                        WarningKind::Unsupported,
                        "purpose",
                        "OpenAI embeddings have no portable task-purpose field",
                    ));
                }
                if request.dimensions.is_some()
                    && request.provider_options.openai.dimensions.is_some()
                {
                    warnings.push(Warning::new(
                        WarningKind::Compatibility,
                        "dimensions",
                        "OpenAI embedding options override portable dimensions",
                    ));
                }
                if request.provider_options.gemini != GeminiEmbeddingOptions::default() {
                    warnings.push(Warning::new(
                        WarningKind::Compatibility,
                        "provider_options.gemini",
                        "Gemini embedding options are not sent to OpenAI",
                    ));
                }
                let size = request
                    .batch_size
                    .map(NonZeroUsize::get)
                    .unwrap_or(2048)
                    .min(2048);
                if request.batch_size.is_some_and(|n| n.get() > 2048) {
                    warnings.push(Warning::new(
                        WarningKind::Compatibility,
                        "batch_size",
                        "OpenAI native batches are limited to 2048 inputs",
                    ));
                }
                for inputs in texts.chunks(size) {
                    let texts = inputs.iter().map(|text| (*text).to_owned()).collect();
                    let options = &request.provider_options.openai;
                    let mut body = o::CreateEmbeddingRequestBody::builder(
                        o::EmbeddingInput::Texts(texts),
                        request.model.id.clone(),
                    )
                    .rest(options.rest.clone())
                    .build();
                    body.dimensions = options
                        .dimensions
                        .or(request.dimensions.map(|n| i64::from(n.get())));
                    body.encoding_format = Some(
                        options
                            .encoding_format
                            .unwrap_or(o::EmbeddingEncodingFormat::Float),
                    );
                    body.user = options.user.clone();
                    let response = self
                        .send(
                            OperationKey {
                                operation: Operation::CreateEmbedding,
                                dialect: protocol::Dialect::OpenAi,
                            },
                            Some(&request.model.id),
                            crate::wire_json(http::Method::POST, "/v1/embeddings".into(), &body)?,
                        )
                        .await?;
                    let batch = read_batch::<o::CreateEmbeddingResponseBody>(
                        response,
                        self.limits,
                        |body| openai_batch(body, inputs.len()),
                    )
                    .await?;
                    vectors.extend(batch.vectors);
                    tokens = add_tokens(tokens, batch.input_tokens)?;
                    warnings.extend(batch.warnings);
                }
            }
            EmbeddingDialect::Gemini => {
                let options = &request.provider_options.gemini;
                if request.provider_options.openai != OpenaiEmbeddingOptions::default() {
                    warnings.push(Warning::new(
                        WarningKind::Compatibility,
                        "provider_options.openai",
                        "OpenAI embedding options are not sent to Gemini",
                    ));
                }
                if request.inputs.len() > 1 && request.batch_size.map(NonZeroUsize::get) != Some(1)
                {
                    warnings.push(Warning::new(
                        WarningKind::Compatibility,
                        "batch_size",
                        "Gemini uses one embedContent call per input so model routing stays in the URL",
                    ));
                }
                let native_task = options.config.as_ref().and_then(|c| c.task_type);
                let native_dims = options
                    .config
                    .as_ref()
                    .and_then(|c| c.output_dimensionality);
                if request.purpose.is_some() && native_task.is_some() {
                    warnings.push(Warning::new(
                        WarningKind::Compatibility,
                        "purpose",
                        "Gemini embedding config overrides portable purpose",
                    ));
                }
                if request.dimensions.is_some() && native_dims.is_some() {
                    warnings.push(Warning::new(
                        WarningKind::Compatibility,
                        "dimensions",
                        "Gemini embedding config overrides portable dimensions",
                    ));
                }
                let task = if native_task.is_none() {
                    purpose(request.purpose.as_ref(), &mut warnings)
                } else {
                    None
                };
                // Compile every part before starting I/O: unsupported inputs do not
                // leave the caller with an accidentally half-submitted batch.
                let contents = request
                    .inputs
                    .iter()
                    .enumerate()
                    .map(|(index, input)| gemini_input(input, index, &mut warnings))
                    .collect::<Result<Vec<_>>>()?;
                for content in contents {
                    let mut body = e::EmbedContentRequestBody::builder(content)
                        .rest(options.rest.clone())
                        .build();
                    body.task_type = task;
                    body.embed_content_config = options.config.clone();
                    if native_dims.is_none() {
                        body.output_dimensionality = request.dimensions.map(|n| i64::from(n.get()));
                    }
                    let path = format!(
                        "/v1beta/models/{}:embedContent",
                        crate::path_segment(&request.model.id)
                    );
                    let response = self
                        .send(
                            OperationKey {
                                operation: Operation::CreateEmbedding,
                                dialect: protocol::Dialect::Gemini,
                            },
                            Some(&request.model.id),
                            crate::wire_json(http::Method::POST, path, &body)?,
                        )
                        .await?;
                    let batch = read_batch::<e::EmbedContentResponseBody>(
                        response,
                        self.limits,
                        gemini_batch,
                    )
                    .await?;
                    vectors.extend(batch.vectors);
                    tokens = add_tokens(tokens, batch.input_tokens)?;
                    warnings.extend(batch.warnings);
                }
            }
            EmbeddingDialect::Unknown(_) => {
                return Err(Error::invalid("unknown embedding dialect"));
            }
        }
        Ok(EmbedResult {
            vectors,
            usage: Usage {
                input_tokens: tokens,
                ..Usage::default()
            },
            warnings,
        })
    }
}
async fn read_batch<T: serde::de::DeserializeOwned>(
    response: WireResponse<HttpBody>,
    limits: protocol::codec::CodecLimits,
    parse: impl FnOnce(T) -> Result<Batch>,
) -> Result<Batch> {
    let status = response.status.as_u16();
    let headers = response.headers;
    let bytes = protocol::codec::read_http_body(response.body, limits)
        .await
        .map_err(|error| Error::from(error).with_head(status, &headers))?;
    serde_json::from_slice::<T>(&bytes)
        .map_err(Error::invalid)
        .and_then(parse)
        .map_err(|error| error.with_head(status, &headers).with_body(&bytes))
}
fn openai_batch(mut body: o::CreateEmbeddingResponseBody, expected: usize) -> Result<Batch> {
    if body.data.len() != expected {
        return Err(Error::invalid(
            "embedding response count differs from the input batch",
        ));
    }
    body.data.sort_by_key(|row| row.index);
    let mut vectors = Vec::with_capacity(expected);
    for (index, row) in body.data.into_iter().enumerate() {
        // Native batches are bounded to 2048, so the usize index fits i64.
        if row.index != index as i64 {
            return Err(Error::invalid(
                "embedding response indices are duplicated, missing or out of range",
            ));
        }
        vectors.push(match row.embedding {
            o::EmbeddingVector::Floats(values) => numbers(values)?,
            o::EmbeddingVector::Base64(encoded) => {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .map_err(Error::invalid)?;
                if bytes.len() % 4 != 0 {
                    return Err(Error::invalid(
                        "base64 embedding is not an f32 byte sequence",
                    ));
                }
                bytes
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|bytes| {
                        let value = f32::from_le_bytes(*bytes) as f64;
                        if value.is_finite() {
                            Ok(value)
                        } else {
                            Err(Error::invalid("embedding contains a non-finite value"))
                        }
                    })
                    .collect::<Result<_>>()?
            }
        });
    }
    Ok(Batch {
        vectors,
        input_tokens: Some(u64::try_from(body.usage.prompt_tokens).map_err(Error::invalid)?),
        warnings: Vec::new(),
    })
}
fn gemini_batch(body: e::EmbedContentResponseBody) -> Result<Batch> {
    let embedding = body
        .embedding
        .ok_or_else(|| Error::invalid("Gemini response has no embedding"))?;
    let warnings = if embedding
        .shape
        .as_ref()
        .is_some_and(|shape| shape.len() > 1)
    {
        vec![Warning::new(
            WarningKind::Compatibility,
            "response.embedding.shape",
            "Tensor-shaped embeddings are returned as their flat values",
        )]
    } else {
        Vec::new()
    };
    Ok(Batch {
        vectors: vec![numbers(
            embedding
                .values
                .ok_or_else(|| Error::invalid("Gemini embedding has no values"))?,
        )?],
        input_tokens: body
            .usage_metadata
            .and_then(|u| u.prompt_token_count)
            .map(u64::try_from)
            .transpose()
            .map_err(Error::invalid)?,
        warnings,
    })
}
fn numbers(values: Vec<serde_json::Number>) -> Result<Vec<f64>> {
    values
        .into_iter()
        .map(|n| {
            n.as_f64()
                .filter(|n| n.is_finite())
                .ok_or_else(|| Error::invalid("embedding value is not a finite number"))
        })
        .collect()
}
fn add_tokens(left: Option<u64>, right: Option<u64>) -> Result<Option<u64>> {
    left.zip(right)
        .map(|(a, b)| {
            a.checked_add(b)
                .ok_or_else(|| Error::invalid("embedding token usage overflow"))
        })
        .transpose()
}
fn purpose(
    value: Option<&EmbeddingPurpose>,
    warnings: &mut Vec<Warning>,
) -> Option<e::GeminiTaskType> {
    use e::GeminiTaskType as T;
    Some(match value? {
        EmbeddingPurpose::Query => T::RetrievalQuery,
        EmbeddingPurpose::Document => T::RetrievalDocument,
        EmbeddingPurpose::SemanticSimilarity => T::SemanticSimilarity,
        EmbeddingPurpose::Classification => T::Classification,
        EmbeddingPurpose::Clustering => T::Clustering,
        EmbeddingPurpose::QuestionAnswering => T::QuestionAnswering,
        EmbeddingPurpose::FactVerification => T::FactVerification,
        EmbeddingPurpose::CodeQuery => T::CodeRetrievalQuery,
        EmbeddingPurpose::Unknown(_) => {
            warnings.push(Warning::new(
                WarningKind::Unsupported,
                "purpose",
                "Unknown embedding purpose is omitted",
            ));
            return None;
        }
    })
}
fn gemini_input(
    input: &EmbeddingInput,
    index: usize,
    warnings: &mut Vec<Warning>,
) -> Result<g::Content> {
    let parts = match input {
        EmbeddingInput::Text(text) => vec![g::Part::builder().text(text.clone()).build()],
        EmbeddingInput::Image(image) => vec![gemini_image(image, index, warnings)?],
        EmbeddingInput::Parts(parts) => parts
            .iter()
            .map(|part| match part {
                EmbeddingPart::Text(text) => Ok(g::Part::builder().text(text.clone()).build()),
                EmbeddingPart::Image(image) => gemini_image(image, index, warnings),
                EmbeddingPart::Unknown(_) => Err(Error::invalid("unknown embedding part")),
            })
            .collect::<Result<_>>()?,
        EmbeddingInput::Unknown(_) => return Err(Error::invalid("unknown embedding input")),
    };
    Ok(g::Content::builder().parts(parts).build())
}
fn gemini_image(image: &Image, index: usize, warnings: &mut Vec<Warning>) -> Result<g::Part> {
    if !matches!(image.source, Asset::ProviderFile { .. }) {
        crate::resolution::check_asset(
            AssetDialect::GeminiEmbedding,
            MediaKind::Image,
            AssetLocation::Embedding,
            &image.source,
            image.media_type.as_deref(),
            None,
        )?;
    }
    if image.detail != Detail::Auto {
        warnings.push(Warning::new(
            WarningKind::Unsupported,
            format!("inputs[{index}].detail"),
            "Embedding image detail is omitted",
        ));
    }
    let mut part = g::Part::builder().build();
    match &image.source {
        Asset::Inline { bytes } => {
            part.inline_data = Some(
                g::Blob::builder(
                    image.media_type.clone().ok_or_else(|| {
                        Error::invalid("inline embedding image requires media_type")
                    })?,
                    base64::engine::general_purpose::STANDARD.encode(bytes),
                )
                .build(),
            )
        }
        Asset::Url { url } => {
            let mut value = g::FileData::builder(url.clone()).build();
            value.mime_type = image.media_type.clone();
            part.file_data = Some(value);
        }
        Asset::ProviderFile {
            vendor: Vendor::Gemini,
            id,
        } => {
            let mut value = g::FileData::builder(id.clone()).build();
            value.mime_type = image.media_type.clone();
            part.file_data = Some(value);
        }
        Asset::ProviderFile { .. } => {
            return Err(Error::invalid("embedding file IDs cannot cross vendors"));
        }
        Asset::Reference { .. } => {
            return Err(Error::invalid(
                "resolve embedding image references before sending",
            ));
        }
        Asset::Unknown(_) => return Err(Error::invalid("unknown embedding image source")),
    }
    Ok(part)
}
