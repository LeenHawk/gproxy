use super::client::*;
use super::*;
use base64::Engine;
use protocol::{Operation, openai::embeddings as o};
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
pub(super) async fn embed<B: Backend>(
    client: &Client<B>,
    request: &EmbedRequest,
    warnings: &mut Vec<Warning>,
    vectors: &mut Vec<Vec<f64>>,
    tokens: &mut Option<u64>,
) -> Result<()> {
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
    if request.dimensions.is_some() && request.provider_options.openai.dimensions.is_some() {
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
        let response = client
            .send(
                OperationKey {
                    operation: Operation::CreateEmbedding,
                    dialect: protocol::Dialect::OpenAi,
                },
                Some(&request.model.id),
                crate::wire_json(http::Method::POST, "/v1/embeddings".into(), &body)?,
            )
            .await?;
        let batch = read_batch::<o::CreateEmbeddingResponseBody>(response, client.limits, |body| {
            openai_batch(body, inputs.len())
        })
        .await?;
        vectors.extend(batch.vectors);
        *tokens = add_tokens(*tokens, batch.input_tokens)?;
        warnings.extend(batch.warnings);
    }

    Ok(())
}
