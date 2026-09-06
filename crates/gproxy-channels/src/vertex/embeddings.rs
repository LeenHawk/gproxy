use bytes::Bytes;
use gproxy_channel_api::{ChannelError, NormalizedUsage, PrepareCtx};
use gproxy_protocol::Operation;
use gproxy_protocol::gemini::{
    BatchEmbedContentsRequest, BatchEmbedContentsResponse, ContentEmbedding, EmbedContentRequest,
    EmbedContentResponse, EmbeddingUsageMetadata,
};
use serde::Deserialize;
use serde_json::{Value, json};

// Legacy text models and batches use predict; embedContent serves newer multimodal models.
pub(super) fn uses_predict(ctx: &PrepareCtx<'_>) -> bool {
    let model = super::model::model_id(ctx.upstream_model);
    ctx.key.operation() == Operation::BatchCreateEmbedding
        || (ctx.key.operation() == Operation::CreateEmbedding
            && (model == "gemini-embedding-001"
                || [
                    "textembedding-",
                    "text-embedding-",
                    "text-multilingual-embedding-",
                ]
                .iter()
                .any(|prefix| model.starts_with(prefix))))
}

pub(super) fn request(ctx: &PrepareCtx<'_>) -> Result<Bytes, ChannelError> {
    let requests = if ctx.key.operation() == Operation::CreateEmbedding {
        vec![serde_json::from_slice::<EmbedContentRequest>(ctx.body).map_err(request_error)?]
    } else {
        serde_json::from_slice::<BatchEmbedContentsRequest>(ctx.body)
            .map_err(request_error)?
            .requests
    };
    if requests.is_empty() {
        return Err(ChannelError::Prepare(
            "Vertex embedding batch is empty".into(),
        ));
    }
    let mut parameters = None;
    let mut instances = Vec::with_capacity(requests.len());
    for item in requests {
        let config = item.embed_content_config.unwrap_or_default();
        let mut instance = json!({"content": item.content.parts.into_iter()
            .map(|part| match part.data {
                Some(gproxy_protocol::gemini::PartData::Text { text, .. }) => Ok(text),
                _ => Err(ChannelError::Prepare("Vertex predict embeddings require text parts".into())),
            })
            .collect::<Result<Vec<_>, _>>()?.concat()});
        if let Some(task) = config.task_type.or(item.task_type) {
            instance["task_type"] = serde_json::to_value(task).map_err(error)?;
        }
        if let Some(title) = config.title.or(item.title) {
            instance["title"] = Value::String(title);
        }
        let mut current = json!({});
        if let Some(dimensions) = config.output_dimensionality.or(item.output_dimensionality) {
            current["outputDimensionality"] = json!(dimensions);
        }
        if let Some(truncate) = config.auto_truncate {
            current["autoTruncate"] = json!(truncate);
        }
        if parameters
            .as_ref()
            .is_some_and(|previous| previous != &current)
        {
            return Err(ChannelError::Prepare(
                "Vertex embedding batch requires matching dimensions and truncation settings"
                    .into(),
            ));
        }
        parameters = Some(current);
        instances.push(instance);
    }
    serde_json::to_vec(&json!({"instances": instances, "parameters": parameters}))
        .map(Bytes::from)
        .map_err(error)
}

pub(super) fn single_request(body: &Bytes) -> Result<Bytes, ChannelError> {
    let mut request: EmbedContentRequest = serde_json::from_slice(body).map_err(request_error)?;
    request.model = None;
    serde_json::to_vec(&request)
        .map(Bytes::from)
        .map_err(request_error)
}

#[derive(Deserialize)]
struct Predictions {
    predictions: Vec<Prediction>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Response {
    Predictions(Predictions),
    Native(EmbedContentResponse),
}

#[derive(Deserialize)]
struct Prediction {
    embeddings: Embedding,
}

#[derive(Deserialize)]
struct Embedding {
    values: Vec<f32>,
    statistics: Statistics,
}

#[derive(Deserialize)]
struct Statistics {
    token_count: u64,
}

impl Predictions {
    fn tokens(&self) -> Option<u64> {
        self.predictions.iter().try_fold(0_u64, |total, item| {
            total.checked_add(item.embeddings.statistics.token_count)
        })
    }
}

pub(super) fn response(body: &Bytes, operation: Operation) -> Result<Bytes, ChannelError> {
    let response: Response = serde_json::from_slice(body).map_err(error)?;
    let response = match response {
        Response::Predictions(response) if !response.predictions.is_empty() => response,
        Response::Native(response)
            if operation == Operation::CreateEmbedding && response.embedding.is_some() =>
        {
            return Ok(body.clone());
        }
        _ => {
            return Err(ChannelError::Decode(
                "Vertex embedding response has no embeddings".into(),
            ));
        }
    };
    let tokens = response
        .tokens()
        .and_then(|tokens| i32::try_from(tokens).ok())
        .ok_or_else(|| ChannelError::Decode("Vertex embedding usage overflow".into()))?;
    let mut response = BatchEmbedContentsResponse {
        embeddings: response
            .predictions
            .into_iter()
            .map(|item| ContentEmbedding {
                values: item.embeddings.values,
                ..Default::default()
            })
            .collect(),
        usage_metadata: Some(EmbeddingUsageMetadata {
            prompt_token_count: Some(tokens),
            ..Default::default()
        }),
        ..Default::default()
    };
    if operation == Operation::CreateEmbedding {
        if response.embeddings.len() != 1 {
            return Err(ChannelError::Decode(
                "Vertex single embedding response has wrong prediction count".into(),
            ));
        }
        return serde_json::to_vec(&EmbedContentResponse {
            embedding: response.embeddings.pop(),
            usage_metadata: response.usage_metadata,
            ..Default::default()
        })
        .map(Bytes::from)
        .map_err(error);
    }
    serde_json::to_vec(&response)
        .map(Bytes::from)
        .map_err(error)
}

pub(super) fn usage(body: &[u8]) -> Option<NormalizedUsage> {
    match serde_json::from_slice::<Response>(body).ok()? {
        Response::Predictions(response) => Some(NormalizedUsage {
            input_tokens: response.tokens()?,
            ..Default::default()
        }),
        Response::Native(response) => {
            crate::shared::gemini::usage::embedding(response.usage_metadata.as_ref()?)
        }
    }
}

fn error(error: serde_json::Error) -> ChannelError {
    ChannelError::Decode(format!("Vertex embedding JSON: {error}"))
}

fn request_error(error: serde_json::Error) -> ChannelError {
    ChannelError::Prepare(format!("Vertex embedding JSON: {error}"))
}
