//! Bytes-level dispatch from a resolved [`TransformPair`] to its typed pair
//! functions. [`content`] holds the 12 content-generation pairs (M2);
//! [`other`] holds count_tokens/models/embeddings/images/compact (M2.5).
//! Streaming is wired for content pairs only.

mod content;
mod other;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::{TransformContext, TransformError, TransformPair};

/// Whether the bytes dispatch has arms for this pair.
pub fn is_wired(pair: TransformPair) -> bool {
    content::is_content(pair) || other::is_wired(pair)
}

/// Convert a request body (inbound wire JSON → upstream wire JSON).
pub fn request_bytes(
    pair: TransformPair,
    ctx: &TransformContext,
    body: &[u8],
) -> Result<Vec<u8>, TransformError> {
    if content::is_content(pair) {
        content::request_bytes(pair, ctx, body)
    } else {
        other::request_bytes(pair, ctx, body)
    }
}

/// Convert a response body (upstream wire JSON → inbound wire JSON). The pair
/// here is the REVERSE pair (`resolve(upstream_key, inbound_key)`).
pub fn response_bytes(
    pair: TransformPair,
    ctx: &TransformContext,
    body: &[u8],
) -> Result<Vec<u8>, TransformError> {
    if content::is_content(pair) {
        content::response_bytes(pair, ctx, body)
    } else {
        other::response_bytes(pair, ctx, body)
    }
}

/// Convert one decoded stream event (upstream wire JSON value → inbound wire
/// JSON value). Same reverse-pair convention as [`response_bytes`]. Only
/// content-generation pairs stream; the other groups are buffered.
pub fn stream_event_value(
    pair: TransformPair,
    ctx: &TransformContext,
    event: Value,
) -> Result<Value, TransformError> {
    if content::is_content(pair) {
        content::stream_event_value(pair, ctx, event)
    } else {
        Err(not_wired(pair))
    }
}

fn run<S, T>(
    f: impl Fn(S, &TransformContext) -> Result<T, TransformError>,
    ctx: &TransformContext,
    body: &[u8],
) -> Result<Vec<u8>, TransformError>
where
    S: DeserializeOwned,
    T: Serialize,
{
    let input: S = serde_json::from_slice(body).map_err(|e| TransformError::InvalidInput {
        reason: format!("decode source body: {e}"),
    })?;
    let out = f(input, ctx)?;
    serde_json::to_vec(&out).map_err(|e| TransformError::Serialization {
        reason: e.to_string(),
    })
}

/// [`run`] for infallible pair functions (plain return, no `Result`).
fn run_ok<S, T>(
    f: impl Fn(S, &TransformContext) -> T,
    ctx: &TransformContext,
    body: &[u8],
) -> Result<Vec<u8>, TransformError>
where
    S: DeserializeOwned,
    T: Serialize,
{
    run(
        |input, ctx| Ok::<_, TransformError>(f(input, ctx)),
        ctx,
        body,
    )
}

fn run_value<S, T>(
    f: impl Fn(S, &TransformContext) -> Result<T, TransformError>,
    ctx: &TransformContext,
    event: Value,
) -> Result<Value, TransformError>
where
    S: DeserializeOwned,
    T: Serialize,
{
    let input: S = serde_json::from_value(event).map_err(|e| TransformError::InvalidInput {
        reason: format!("decode stream event: {e}"),
    })?;
    let out = f(input, ctx)?;
    serde_json::to_value(&out).map_err(|e| TransformError::Serialization {
        reason: e.to_string(),
    })
}

fn not_wired(pair: TransformPair) -> TransformError {
    TransformError::InvalidInput {
        reason: format!("bytes dispatch not wired for {pair:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ContentGenerationKind, Operation, OperationKey, Provider};

    #[test]
    fn claude_to_openai_chat_request_roundtrip() {
        let source = OperationKey::content_generation(
            Operation::GenerateContent,
            ContentGenerationKind::ClaudeMessages,
        );
        let target = OperationKey::content_generation(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiChatCompletions,
        );
        let ctx = TransformContext::new(source, target);
        let body = br#"{"model":"m","max_tokens":16,"messages":[{"role":"user","content":"hi"}]}"#;
        let out = request_bytes(TransformPair::ClaudeMessagesToOpenAiChat, &ctx, body).unwrap();
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["messages"][0]["role"], "user");
        assert!(v.get("max_tokens").is_some() || v.get("max_completion_tokens").is_some());
    }

    #[test]
    fn claude_to_openai_count_tokens_request_roundtrip() {
        let source = OperationKey::provider(Operation::CountTokens, Provider::Claude);
        let target = OperationKey::provider(Operation::CountTokens, Provider::OpenAi);
        let ctx = TransformContext::new(source, target);
        let body = br#"{"model":"m","messages":[{"role":"user","content":"hi"}]}"#;
        let out = request_bytes(TransformPair::ClaudeToOpenAiCountTokens, &ctx, body).unwrap();
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["model"], "m");
        assert!(v.get("input").is_some());
    }

    #[test]
    fn openai_to_claude_models_list_response_roundtrip() {
        let source = OperationKey::provider(Operation::ListModels, Provider::OpenAi);
        let target = OperationKey::provider(Operation::ListModels, Provider::Claude);
        let ctx = TransformContext::new(source, target);
        let body = br#"{"object":"list","data":[{"id":"gpt-x","created":1,"object":"model","owned_by":"openai"}]}"#;
        let out = response_bytes(TransformPair::OpenAiToClaudeModels, &ctx, body).unwrap();
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["data"][0]["id"], "gpt-x");
    }
}
