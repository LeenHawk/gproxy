use super::client::*;
use super::*;
use base64::Engine;
use protocol::{
    Operation,
    gemini::{self as g, embeddings as e},
};
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
        crate::assets::resolve::check_asset(
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
pub(super) async fn embed<B: Backend>(
    client: &Client<B>,
    request: &EmbedRequest,
    warnings: &mut Vec<Warning>,
    vectors: &mut Vec<Vec<f64>>,
    tokens: &mut Option<u64>,
) -> Result<()> {
    let options = &request.provider_options.gemini;
    if request.provider_options.openai != OpenaiEmbeddingOptions::default() {
        warnings.push(Warning::new(
            WarningKind::Compatibility,
            "provider_options.openai",
            "OpenAI embedding options are not sent to Gemini",
        ));
    }
    if request.inputs.len() > 1 && request.batch_size.map(NonZeroUsize::get) != Some(1) {
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
        purpose(request.purpose.as_ref(), warnings)
    } else {
        None
    };
    // Compile every part before starting I/O: unsupported inputs do not
    // leave the caller with an accidentally half-submitted batch.
    let contents = request
        .inputs
        .iter()
        .enumerate()
        .map(|(index, input)| gemini_input(input, index, warnings))
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
        let response = client
            .send(
                OperationKey {
                    operation: Operation::CreateEmbedding,
                    dialect: protocol::Dialect::Gemini,
                },
                Some(&request.model.id),
                crate::wire_json(http::Method::POST, path, &body)?,
            )
            .await?;
        let batch =
            read_batch::<e::EmbedContentResponseBody>(response, client.limits, gemini_batch)
                .await?;
        vectors.extend(batch.vectors);
        *tokens = add_tokens(*tokens, batch.input_tokens)?;
        warnings.extend(batch.warnings);
    }

    Ok(())
}
