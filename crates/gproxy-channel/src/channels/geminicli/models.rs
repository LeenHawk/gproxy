//! The model directory, synthesized from the per-account quota buckets.
//!
//! Code Assist has no model list of its own; the CLI learns which models an
//! account may call from `POST /v1internal:retrieveUserQuota`, whose
//! `buckets` carry a `modelId` per `tokenType` (v3 `geminicli/models.rs`).
//! Only the `REQUESTS` buckets name callable models.

use super::GeminiCli;
use crate::channel::{BaseChannel, ChannelError, OperationContext, PrepareContext};
use crate::channels::shared::code_assist;
use gproxy_protocol::{HttpBody, Operation, OperationKey, WireResponse};
use http::{StatusCode, header};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// The generation methods every Code Assist model serves.
const METHODS: [&str; 3] = ["countTokens", "generateContent", "streamGenerateContent"];

pub(super) fn catalog(body: &[u8]) -> Result<Vec<Value>, ChannelError> {
    let payload: Value = serde_json::from_slice(body).map_err(|error| {
        code_assist::invalid_response(format!("Code Assist quota response JSON: {error}"))
    })?;
    let mut ids = BTreeSet::new();
    for bucket in payload
        .get("buckets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if bucket.get("tokenType").and_then(Value::as_str) != Some("REQUESTS") {
            continue;
        }
        if let Some(id) = code_assist::text(bucket, "modelId") {
            ids.insert(code_assist::model_id(id).to_owned());
        }
    }
    Ok(ids
        .into_iter()
        .map(|id| {
            json!({
                "name": format!("models/{id}"),
                "baseModelId": id,
                "supportedGenerationMethods": METHODS,
            })
        })
        .collect())
}

/// The model id a `GET /v1beta/models/{id}` path names.
fn requested(path: &str) -> Result<String, ChannelError> {
    code_assist::request_model(path, None)
        .ok_or_else(|| ChannelError::InvalidConfig("model id missing from request path".into()))
}

pub(super) async fn invoke(
    channel: &GeminiCli,
    operation: Operation,
    ctx: OperationContext<'_>,
) -> Result<WireResponse<HttpBody>, ChannelError> {
    let wanted = match operation {
        Operation::GetModel => Some(requested(&ctx.request.path)?),
        _ => None,
    };
    let OperationContext {
        provider,
        credential,
        dialect,
        request,
        client,
        endpoint_override,
        ..
    } = ctx;
    // `retrieveUserQuota` is a POST with its own body; a client's list or
    // get request carries none, so preparation replaces it either way.
    let prepared = channel.prepare(PrepareContext {
        provider,
        credential,
        operation: OperationKey { operation, dialect },
        request,
        endpoint_override,
    })?;
    let mut response = client.send(prepared).await?;
    if !response.status.is_success() {
        return Ok(response);
    }
    let bytes = code_assist::read_body(response.body).await?;
    let models = catalog(&bytes)?;
    let body = match wanted {
        None => code_assist::encode(&json!({ "models": models }), code_assist::invalid_response)?,
        Some(id) => match models
            .into_iter()
            .find(|model| model["baseModelId"] == Value::String(id.clone()))
        {
            Some(model) => code_assist::encode(&model, code_assist::invalid_response)?,
            None => {
                response.status = StatusCode::NOT_FOUND;
                code_assist::encode(
                    &json!({"error": {
                        "code": 404,
                        "status": "NOT_FOUND",
                        "message": format!("Model `{id}` is not available to this account"),
                    }}),
                    code_assist::invalid_response,
                )?
            }
        },
    };
    response.headers.remove(header::CONTENT_LENGTH);
    response.headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json"),
    );
    response.body = HttpBody::Bytes(body);
    Ok(response)
}
