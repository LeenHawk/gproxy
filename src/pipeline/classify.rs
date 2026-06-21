//! Inbound request classification: `(method, path)` → [`OperationKey`] plus the
//! streaming flag. M1 ships a hardcoded table (D5); unknown rows → 404.

use bytes::Bytes;
use http::{HeaderMap, Method};

use crate::pipeline::context::Classified;
use crate::pipeline::error::PipelineError;
use crate::protocol::{ContentGenerationKind as CGK, Operation, OperationKey, Provider as Prov};

/// Classify by `(method, path)`. The leading `/v1` is present in both aggregated
/// and (post-strip) scoped paths. Headers disambiguate the shared `/v1/models`
/// surface (Claude callers send `x-api-key`).
pub fn classify(
    method: &Method,
    path: &str,
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<Classified, PipelineError> {
    let (op, stream) = match (method.as_str(), path) {
        ("POST", "/v1/chat/completions") => (
            OperationKey::content_generation(
                Operation::GenerateContent,
                CGK::OpenAiChatCompletions,
            ),
            peek_stream(body),
        ),
        ("POST", "/v1/responses") => (
            OperationKey::content_generation(Operation::GenerateContent, CGK::OpenAiResponses),
            peek_stream(body),
        ),
        ("POST", "/v1/messages") => (
            OperationKey::content_generation(Operation::GenerateContent, CGK::ClaudeMessages),
            peek_stream(body),
        ),
        ("POST", "/v1/messages/count_tokens") => (
            OperationKey::provider(Operation::CountTokens, Prov::Claude),
            false,
        ),
        ("POST", "/v1/responses/input_tokens") => (
            OperationKey::provider(Operation::CountTokens, Prov::OpenAi),
            false,
        ),
        ("POST", "/v1/embeddings") => (
            OperationKey::provider(Operation::CreateEmbedding, Prov::OpenAi),
            false,
        ),
        ("POST", "/v1/images/generations") => (
            OperationKey::provider(Operation::CreateImage, Prov::OpenAi),
            false,
        ),
        ("GET", "/v1/models") => (
            OperationKey::provider(Operation::ListModels, credential_provider(headers)),
            false,
        ),
        ("GET", "/v1beta/models") => (
            OperationKey::provider(Operation::ListModels, Prov::Gemini),
            false,
        ),
        ("GET", p) => match get_model(p, headers) {
            Some(key) => (key, false),
            None => return Err(PipelineError::UnsupportedPath),
        },
        ("POST", p) => match gemini_suffix(p) {
            Some(key_stream) => key_stream,
            None => return Err(PipelineError::UnsupportedPath),
        },
        _ => return Err(PipelineError::UnsupportedPath),
    };
    Ok(Classified { op, stream })
}

/// Credential form on the shared OpenAI/Claude path surface: Claude clients
/// authenticate with `x-api-key`, OpenAI clients with `authorization`.
fn credential_provider(headers: &HeaderMap) -> Prov {
    if headers.contains_key("x-api-key") {
        Prov::Claude
    } else {
        Prov::OpenAi
    }
}

/// `GET /v1/models/{id}` (OpenAI/Claude, by credential form) and
/// `GET /v1beta/models/{id}` (gemini; a `:verb` suffix means a content path,
/// never GetModel).
fn get_model(path: &str, headers: &HeaderMap) -> Option<OperationKey> {
    if let Some(rest) = path.strip_prefix("/v1/models/") {
        if !rest.is_empty() && !rest.contains('/') {
            return Some(OperationKey::provider(
                Operation::GetModel,
                credential_provider(headers),
            ));
        }
    } else if let Some(rest) = path.strip_prefix("/v1beta/models/")
        && !rest.is_empty()
        && !rest.contains(':')
    {
        return Some(OperationKey::provider(Operation::GetModel, Prov::Gemini));
    }
    None
}

/// Gemini `…/models/{model}:verb`, matched on the path suffix after the last
/// `/` (independent of `{model}`).
fn gemini_suffix(path: &str) -> Option<(OperationKey, bool)> {
    let last = path.rsplit('/').next()?;
    if last.ends_with(":streamGenerateContent") {
        Some((
            OperationKey::content_generation(
                Operation::StreamGenerateContent,
                CGK::GeminiGenerateContent,
            ),
            true,
        ))
    } else if last.ends_with(":generateContent") {
        Some((
            OperationKey::content_generation(
                Operation::GenerateContent,
                CGK::GeminiGenerateContent,
            ),
            false,
        ))
    } else if last.ends_with(":countTokens") {
        Some((
            OperationKey::provider(Operation::CountTokens, Prov::Gemini),
            false,
        ))
    } else {
        None
    }
}

/// Minimal body peek for the `"stream"` flag (NOT a full protocol deserialize).
/// Tolerant: a type error elsewhere in the body must not flip the flag, and a
/// non-bool `stream` is treated as absent.
pub(crate) fn peek_stream(body: &Bytes) -> bool {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("stream").and_then(serde_json::Value::as_bool))
        .unwrap_or(false)
}

/// Minimal body peek for the `"model"` field (tolerant, as above).
pub(crate) fn peek_model(body: &Bytes) -> Option<String> {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("model").and_then(|m| m.as_str().map(str::to_string)))
}

/// Model id embedded in the path (gemini `models/{id}:verb`, `/v1/models/{id}`).
/// Only matches a `/models/{id}` segment — non-model paths return `None`.
pub(crate) fn path_model_id(path: &str) -> Option<String> {
    let (_, rest) = path.rsplit_once("/models/")?;
    if rest.is_empty() || rest.contains('/') {
        return None;
    }
    let id = rest.split(':').next().unwrap_or(rest);
    (!id.is_empty()).then(|| id.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::OperationKind;

    fn op(c: &Classified) -> (Operation, OperationKind) {
        (c.op.operation, c.op.kind)
    }

    #[test]
    fn chat_completions_streaming() {
        let body = Bytes::from_static(b"{\"model\":\"x\",\"stream\":true}");
        let c = classify(
            &Method::POST,
            "/v1/chat/completions",
            &HeaderMap::new(),
            &body,
        )
        .unwrap();
        assert!(c.stream);
    }

    #[test]
    fn gemini_stream_suffix() {
        let body = Bytes::new();
        let c = classify(
            &Method::POST,
            "/v1beta/models/gemini-pro:streamGenerateContent",
            &HeaderMap::new(),
            &body,
        )
        .unwrap();
        assert!(c.stream);
    }

    #[test]
    fn unknown_path_is_unsupported() {
        let body = Bytes::new();
        assert!(matches!(
            classify(&Method::POST, "/v1/nope", &HeaderMap::new(), &body),
            Err(PipelineError::UnsupportedPath)
        ));
    }

    #[test]
    fn models_header_disambiguation() {
        let body = Bytes::new();
        let mut claude = HeaderMap::new();
        claude.insert("x-api-key", "sk".parse().unwrap());
        let c = classify(&Method::GET, "/v1/models", &claude, &body).unwrap();
        assert_eq!(
            op(&c),
            (Operation::ListModels, OperationKind::Provider(Prov::Claude))
        );
        let c = classify(&Method::GET, "/v1/models/gpt-x", &HeaderMap::new(), &body).unwrap();
        assert_eq!(
            op(&c),
            (Operation::GetModel, OperationKind::Provider(Prov::OpenAi))
        );
        assert!(classify(&Method::GET, "/v1/models/a/b", &HeaderMap::new(), &body).is_err());
    }

    #[test]
    fn count_tokens_paths() {
        let body = Bytes::new();
        let h = HeaderMap::new();
        for (path, prov) in [
            ("/v1/messages/count_tokens", Prov::Claude),
            ("/v1/responses/input_tokens", Prov::OpenAi),
            ("/v1beta/models/gemini-pro:countTokens", Prov::Gemini),
        ] {
            let c = classify(&Method::POST, path, &h, &body).unwrap();
            assert_eq!(
                op(&c),
                (Operation::CountTokens, OperationKind::Provider(prov))
            );
            assert!(!c.stream);
        }
    }

    #[test]
    fn gemini_models_paths() {
        let body = Bytes::new();
        let h = HeaderMap::new();
        let c = classify(&Method::GET, "/v1beta/models", &h, &body).unwrap();
        assert_eq!(
            op(&c),
            (Operation::ListModels, OperationKind::Provider(Prov::Gemini))
        );
        let c = classify(&Method::GET, "/v1beta/models/gemini-pro", &h, &body).unwrap();
        assert_eq!(
            op(&c),
            (Operation::GetModel, OperationKind::Provider(Prov::Gemini))
        );
        // `:verb` suffix is a content path, never GetModel
        assert!(classify(&Method::GET, "/v1beta/models/g:generateContent", &h, &body).is_err());
    }
}
