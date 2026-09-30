//! Embeddings across the OpenAI and Gemini wires. OpenAI has one `create`
//! call whose input may be a list; Gemini has `embedContent` for one input
//! and `batchEmbedContents` for many. A list on either side is split and
//! packed by protocol's batch adapters over the attempt-bound upstream.
//! Claude has no embeddings API, so any pair touching it is unsupported.

use super::{Call, Converted, endpoints::embedding_template};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireResponse,
    adapt::embeddings::{
        EmbeddingBatchError, EmbeddingBatchOptions, EmbeddingFailure, gemini_batch_to_openai,
        gemini_single_to_openai, openai_to_gemini_batch, openai_to_gemini_single,
    },
    codec::{CodecLimits, decode_json, encode_json},
    transform::{TransformError, TransformErrorKind},
    wire::{gemini::embeddings as g, openai::embeddings as o},
};
use gproxy_seaorm::BatchConnectionTrait;
use http::{HeaderMap, HeaderValue, StatusCode, header::CONTENT_TYPE};
use serde::{Serialize, de::DeserializeOwned};

fn codec(error: gproxy_protocol::codec::CodecError) -> TransformError {
    TransformError::with_source(
        if error.kind() == gproxy_protocol::codec::CodecErrorKind::Limit {
            TransformErrorKind::Limit
        } else {
            TransformErrorKind::InvalidInput
        },
        "client.body",
        error.to_string(),
        error,
    )
}

fn decode<T: DeserializeOwned>(body: &[u8], limits: CodecLimits) -> Result<T, TransformError> {
    decode_json(body, limits).map_err(codec)
}

fn success<T: Serialize>(body: &T, limits: CodecLimits) -> Result<Converted, TransformError> {
    let bytes = encode_json(body, limits).map_err(codec)?;
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    Ok(Converted::Success(WireResponse {
        status: StatusCode::OK,
        headers,
        body: HttpBody::Bytes(bytes),
    }))
}

fn finish<T: Serialize>(
    result: Result<
        gproxy_protocol::adapt::embeddings::EmbeddingBatchResult<T>,
        EmbeddingBatchError,
    >,
    limits: CodecLimits,
) -> Result<Converted, TransformError> {
    match result {
        Ok(result) => success(&result.output.value, limits),
        Err(EmbeddingBatchError {
            failure: EmbeddingFailure::Rejected(response),
            ..
        }) => Ok(Converted::Rejected(*response)),
        Err(EmbeddingBatchError {
            failure: EmbeddingFailure::Transform(error),
            ..
        }) => Err(error),
    }
}

fn options(limits: CodecLimits) -> EmbeddingBatchOptions {
    EmbeddingBatchOptions {
        codec: limits,
        usage_per_call: Vec::new(),
    }
}

fn is_openai(dialect: Dialect) -> bool {
    matches!(dialect, Dialect::OpenAi | Dialect::OpenAiChat)
}

fn key(operation: Operation, dialect: Dialect) -> OperationKey {
    OperationKey { operation, dialect }
}

pub(crate) async fn run<C: BatchConnectionTrait + Send + Sync>(
    call: &Call<'_, C>,
) -> Result<Converted, TransformError> {
    let client = call.client.dialect;
    let target = call.target;
    let upstream = call.upstream;
    let limits = call.limits;
    let model = call.model()?;
    let unsupported = || {
        TransformError::unsupported(
            "embeddings",
            format!(
                "no conversion of {:?} from {client:?} to {target:?}",
                call.client.operation
            ),
        )
    };
    // Adapter futures are boxed: the attempt loop polls this family alongside
    // every other on one stack frame.
    match call.client.operation {
        Operation::CreateEmbedding if is_openai(client) && target == Dialect::Gemini => {
            let input: o::CreateEmbeddingRequestBody = decode(call.body(), limits)?;
            match input.input {
                o::EmbeddingInput::Text(_) | o::EmbeddingInput::Tokens(_) => {
                    let template = embedding_template(target, model, false)?;
                    let result = Box::pin(openai_to_gemini_single(
                        upstream,
                        &key(Operation::CreateEmbedding, target),
                        template,
                        input,
                        model,
                        limits,
                        // Token counts are not known here; Gemini's own
                        // usage metadata is the only source.
                        None,
                    ))
                    .await;
                    finish(result, limits)
                }
                _ => {
                    let template = embedding_template(target, model, true)?;
                    let result = Box::pin(openai_to_gemini_batch(
                        upstream,
                        &key(Operation::BatchCreateEmbedding, target),
                        template,
                        input,
                        model,
                        options(limits),
                    ))
                    .await;
                    finish(result, limits)
                }
            }
        }
        Operation::CreateEmbedding if client == Dialect::Gemini && is_openai(target) => {
            let input: g::EmbedContentRequestBody = decode(call.body(), limits)?;
            let template = embedding_template(target, model, false)?;
            let result = Box::pin(gemini_single_to_openai(
                upstream,
                &key(Operation::CreateEmbedding, target),
                template,
                input,
                model,
                limits,
            ))
            .await;
            finish(result, limits)
        }
        Operation::BatchCreateEmbedding if client == Dialect::Gemini && is_openai(target) => {
            let input: g::BatchEmbedContentsRequestBody = decode(call.body(), limits)?;
            let template = embedding_template(target, model, false)?;
            let result = Box::pin(gemini_batch_to_openai(
                upstream,
                &key(Operation::CreateEmbedding, target),
                template,
                input,
                model,
                options(limits),
            ))
            .await;
            finish(result, limits)
        }
        _ => Err(unsupported()),
    }
}
