//! Bytes-level dispatch from a resolved [`TransformPair`] to its typed pair
//! functions. The private `content` module holds content-generation pairs;
//! `other` holds count_tokens/models/embeddings/images/compact.
//! Streaming is wired for content pairs only.

mod content;
mod other;

use serde::Serialize;
use serde::de::DeserializeOwned;

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
    validate_pair(pair, ctx)?;
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
    validate_pair(pair, ctx)?;
    if content::is_content(pair) {
        content::response_bytes(pair, ctx, body)
    } else {
        other::response_bytes(pair, ctx, body)
    }
}

/// One converted stream event: pre-encoded inbound frame payload, or the
/// typed Responses event when the inbound side runs the aggregation state
/// machine.
pub enum StreamEventOut {
    Encoded { event: Option<String>, data: String },
    Responses(Box<crate::protocol::openai::ResponseStreamEvent>),
}

/// Stateful `0..N` stream-event converter for one resolved pair.
///
/// Create one converter per upstream response and retain it until
/// [`finish`](Self::finish). This preserves pair-specific state such as tool
/// call arguments split across multiple frames.
pub struct StreamConverter {
    inner: content::ContentStreamConverter,
}

impl StreamConverter {
    pub fn new(pair: TransformPair, ctx: TransformContext) -> Result<Self, TransformError> {
        validate_pair(pair, &ctx)?;
        Ok(Self {
            inner: content::ContentStreamConverter::new(pair, ctx)?,
        })
    }

    /// Convert one decoded upstream event into zero or more inbound events.
    pub fn push(&mut self, data: &str) -> Result<Vec<StreamEventOut>, TransformError> {
        self.inner.push(data)
    }

    /// Flush pair-specific state into zero or more final inbound events.
    pub fn finish(&mut self) -> Result<Vec<StreamEventOut>, TransformError> {
        self.inner.finish()
    }
}

/// Convert one decoded stream event (upstream wire JSON text → inbound event).
/// Same reverse-pair convention as [`response_bytes`]. Only content-generation
/// pairs stream; the other groups are buffered. This convenience call retains
/// no cross-frame state; use [`StreamConverter`] for an actual response stream.
pub fn stream_event(
    pair: TransformPair,
    ctx: &TransformContext,
    data: &str,
) -> Result<Vec<StreamEventOut>, TransformError> {
    if content::is_content(pair) {
        let mut converter = StreamConverter::new(pair, ctx.clone())?;
        converter.push(data)
    } else {
        Err(not_wired(pair))
    }
}

fn validate_pair(pair: TransformPair, ctx: &TransformContext) -> Result<(), TransformError> {
    let resolved = super::resolve(ctx.source, ctx.target)?;
    if resolved == pair {
        Ok(())
    } else {
        Err(TransformError::InvalidInput {
            reason: format!(
                "transform pair {pair:?} does not match context {:?} -> {:?} (resolved {resolved:?})",
                ctx.source, ctx.target
            ),
        })
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

fn not_wired(pair: TransformPair) -> TransformError {
    TransformError::InvalidInput {
        reason: format!("bytes dispatch not wired for {pair:?}"),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

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
    fn openai_responses_to_websocket_request_roundtrip() {
        let source = OperationKey::content_generation(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiResponses,
        );
        let target = OperationKey::content_generation(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiResponsesWebSocket,
        );
        let pair = crate::transform::resolve(source, target).unwrap();
        assert!(is_wired(pair));

        let ctx = TransformContext::new(source, target);
        let body = br#"{"model":"m","input":"hi","stream":true}"#;
        let out = request_bytes(pair, &ctx, body).unwrap();
        let v: Value = serde_json::from_slice(&out).unwrap();

        assert_eq!(v["type"], "response.create");
        assert_eq!(v["model"], "m");
        assert_eq!(v["stream"], true);
    }

    #[test]
    fn claude_to_openai_responses_websocket_request_roundtrip() {
        let source = OperationKey::content_generation(
            Operation::GenerateContent,
            ContentGenerationKind::ClaudeMessages,
        );
        let target = OperationKey::content_generation(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiResponsesWebSocket,
        );
        let pair = crate::transform::resolve(source, target).unwrap();
        assert!(is_wired(pair));

        let ctx = TransformContext::new(source, target);
        let body = br#"{"model":"m","max_tokens":16,"messages":[{"role":"user","content":"hi"}]}"#;
        let out = request_bytes(pair, &ctx, body).unwrap();
        let v: Value = serde_json::from_slice(&out).unwrap();

        assert_eq!(v["type"], "response.create");
        assert_eq!(v["model"], "m");
        assert!(v.get("input").is_some());
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
    fn compact_to_responses_is_resolved_and_wired() {
        let source = OperationKey::provider(Operation::CompactContent, Provider::OpenAi);
        let target = OperationKey::content_generation(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiResponses,
        );
        let pair = crate::transform::resolve(source, target).unwrap();
        assert_eq!(pair, TransformPair::OpenAiCompactToOpenAiResponses);
        assert!(is_wired(pair));

        let ctx = TransformContext::new(source, target);
        let body = br#"{"model":"m","input":"summarize this"}"#;
        let out = request_bytes(pair, &ctx, body).unwrap();
        let value: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(value["model"], "m");
        assert!(value.get("input").is_some());
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
