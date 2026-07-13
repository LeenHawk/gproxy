//! Native Anthropic Messages mapping for Vertex AI partner models.

use bytes::Bytes;
use serde_json::Value;

use crate::channel::{ChannelError, ShapeCtx};
use crate::protocol::{ContentGenerationKind, Operation, OperationKind, Provider};

const ANTHROPIC_VERSION: &str = "vertex-2023-10-16";
const MESSAGES_PATH: &str = "/v1/messages";
const COUNT_TOKENS_PATH: &str = "/v1/messages/count_tokens";

pub(super) fn is_native_path(path: &str) -> bool {
    matches!(path, MESSAGES_PATH | COUNT_TOKENS_PATH)
}

/// Convert an Anthropic-compatible path to Vertex's partner-model endpoint.
pub(super) fn target_path(
    path: &str,
    body: &Bytes,
    project_id: &str,
    location: &str,
    model: &str,
) -> Result<Option<String>, ChannelError> {
    let suffix = match path {
        MESSAGES_PATH => {
            if model.trim().is_empty() {
                return Err(ChannelError::Build(
                    "Vertex Claude request is missing a model".into(),
                ));
            }
            let verb = if request_streams(body) {
                "streamRawPredict"
            } else {
                "rawPredict"
            };
            format!("{model}:{verb}")
        }
        COUNT_TOKENS_PATH => "count-tokens:rawPredict".to_owned(),
        _ => return Ok(None),
    };
    Ok(Some(format!(
        "/v1/projects/{project_id}/locations/{location}/publishers/anthropic/models/{suffix}"
    )))
}

/// Vertex carries the Anthropic API version in the JSON body. Messages also
/// carry their model in the URL, while count_tokens keeps it in the body.
pub(super) fn shape_request(body: Bytes, ctx: &ShapeCtx) -> Bytes {
    let messages = matches!(
        ctx.op.kind,
        OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
    );
    let count_tokens = ctx.op.operation == Operation::CountTokens
        && ctx.op.kind == OperationKind::Provider(Provider::Claude);
    if !messages && !count_tokens {
        return body;
    }
    crate::channel::shaping::with_json_body(body, |value| {
        let Some(map) = value.as_object_mut() else {
            return;
        };
        let has_version = map
            .get("anthropic_version")
            .and_then(Value::as_str)
            .is_some_and(|version| !version.is_empty());
        if !has_version {
            map.insert(
                "anthropic_version".to_owned(),
                Value::String(ANTHROPIC_VERSION.to_owned()),
            );
        }
        if messages {
            map.remove("model");
        }
    })
}

fn request_streams(body: &Bytes) -> bool {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| value.get("stream").and_then(Value::as_bool))
        .unwrap_or(false)
}
