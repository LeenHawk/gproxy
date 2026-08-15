//! Inbound request classification: `(method, path)` → [`OperationKey`] plus the
//! streaming flag. M1 ships a hardcoded table (D5); unknown rows → 404.

#[cfg(test)]
mod audio_tests;
mod conversation;
#[cfg(test)]
mod video_tests;

use bytes::Bytes;
use http::{HeaderMap, Method};

use crate::pipeline::context::Classified;
use crate::pipeline::error::PipelineError;
use crate::protocol::{ContentGenerationKind as CGK, Operation, OperationKey, Provider as Prov};

pub(crate) const RESPONSES_WEBSOCKET_CLASSIFY_HEADER: &str = "x-gproxy-responses-websocket";

/// Classify by `(method, path)`. The leading `/v1` is present in both aggregated
/// and (post-strip) scoped paths. Headers disambiguate the shared `/v1/models`
/// surface (Claude callers send `x-api-key`).
pub fn classify(
    method: &Method,
    path: &str,
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<Classified, PipelineError> {
    // ONE tolerant body parse per request: every downstream consumer (routing
    // preprocess, transform memo, rule filters, local count, and conversation
    // affinity) reads facts derived from this value instead of re-parsing.
    let body_value = (method == Method::POST && !body.is_empty())
        .then(|| serde_json::from_slice::<serde_json::Value>(body).ok())
        .flatten();
    let (body_stream, body_model) = body_value.as_ref().map(peek_body).unwrap_or((false, None));
    // Multipart image edits are canonicalized before classification. Their
    // text fields intentionally remain strings, so accept the same `"true"`
    // form that the image request decoder already supports.
    let image_stream = body_stream
        || body_value
            .as_ref()
            .and_then(|value| value.get("stream"))
            .and_then(serde_json::Value::as_str)
            .and_then(|value| value.parse::<bool>().ok())
            .unwrap_or(false);
    let (op, stream) = match (method.as_str(), path) {
        ("POST", "/v1/chat/completions") => (
            OperationKey::content_generation(
                content_operation(body_stream),
                CGK::OpenAiChatCompletions,
            ),
            body_stream,
        ),
        ("POST", "/v1/responses") => {
            let websocket = headers.contains_key(RESPONSES_WEBSOCKET_CLASSIFY_HEADER);
            let stream = websocket || body_stream;
            (
                OperationKey::content_generation(
                    content_operation(stream),
                    if websocket {
                        CGK::OpenAiResponsesWebSocket
                    } else {
                        CGK::OpenAiResponses
                    },
                ),
                stream,
            )
        }
        ("POST", "/v1/messages") => (
            OperationKey::content_generation(content_operation(body_stream), CGK::ClaudeMessages),
            body_stream,
        ),
        ("POST", "/v1/messages/count_tokens") => (
            OperationKey::provider(Operation::CountTokens, Prov::Claude),
            false,
        ),
        ("POST", "/v1/responses/input_tokens") => (
            OperationKey::provider(Operation::CountTokens, Prov::OpenAi),
            false,
        ),
        ("POST", "/v1/responses/compact") => (
            OperationKey::provider(Operation::CompactContent, Prov::OpenAi),
            false,
        ),
        ("POST", "/v1/embeddings") => (
            OperationKey::provider(Operation::CreateEmbedding, Prov::OpenAi),
            false,
        ),
        ("POST", "/v1/audio/speech") => (
            OperationKey::provider(Operation::CreateSpeech, Prov::OpenAi),
            body_value
                .as_ref()
                .and_then(|value| value.get("stream_format"))
                .and_then(serde_json::Value::as_str)
                == Some("sse"),
        ),
        ("POST", "/v1/audio/transcriptions") => (
            OperationKey::provider(Operation::CreateTranscription, Prov::OpenAi),
            body_value
                .as_ref()
                .and_then(|value| value.get("stream"))
                .is_some_and(json_bool),
        ),
        ("POST", "/v1/audio/translations") => (
            OperationKey::provider(Operation::CreateTranslation, Prov::OpenAi),
            false,
        ),
        ("POST", "/v1/rerank") => (
            OperationKey::provider(Operation::Rerank, Prov::OpenAi),
            false,
        ),
        ("POST", "/v1/images/generations") => (
            OperationKey::provider(Operation::CreateImage, Prov::OpenAi),
            image_stream,
        ),
        ("POST", "/v1/images/edits") => (
            OperationKey::provider(Operation::EditImage, Prov::OpenAi),
            image_stream,
        ),
        (method, path) if video_operation(method, path).is_some() => (
            OperationKey::provider(
                video_operation(method, path).expect("guard matched"),
                Prov::OpenAi,
            ),
            false,
        ),
        ("POST", "/v1/alpha/search") => (
            OperationKey::provider(Operation::WebSearch, Prov::OpenAi),
            false,
        ),
        ("POST", "/v1/realtime/calls") => (
            OperationKey::provider(Operation::CreateRealtimeCall, Prov::OpenAi),
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
        ("GET", "/v1/realtime" | "/v1/live") => (
            OperationKey::provider(Operation::ConnectRealtime, Prov::OpenAi),
            true,
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
    let conversation_fingerprint = body_value
        .as_ref()
        .and_then(|body| conversation::fingerprint(op, body));
    Ok(Classified {
        op,
        stream,
        body_model,
        conversation_fingerprint,
    })
}

fn video_operation(method: &str, path: &str) -> Option<Operation> {
    match (method, path) {
        ("POST", "/v1/videos") => Some(Operation::CreateVideo),
        ("GET", "/v1/videos") => Some(Operation::ListVideos),
        ("POST", "/v1/videos/characters") => Some(Operation::CreateVideoCharacter),
        ("POST", "/v1/videos/edits") => Some(Operation::EditVideo),
        ("POST", "/v1/videos/extensions") => Some(Operation::ExtendVideo),
        ("GET", path) => video_resource_operation(path, "GET"),
        ("POST", path) => video_resource_operation(path, "POST"),
        ("DELETE", path) => video_resource_operation(path, "DELETE"),
        _ => None,
    }
}

fn video_resource_operation(path: &str, method: &str) -> Option<Operation> {
    if let Some(character_id) = path.strip_prefix("/v1/videos/characters/")
        && !character_id.is_empty()
        && !character_id.contains('/')
    {
        return (method == "GET").then_some(Operation::GetVideoCharacter);
    }

    let resource = path.strip_prefix("/v1/videos/")?;
    let mut segments = resource.split('/');
    let video_id = segments.next()?;
    if video_id.is_empty() {
        return None;
    }
    match (method, segments.next(), segments.next()) {
        ("GET", None, None) => Some(Operation::RetrieveVideo),
        ("DELETE", None, None) => Some(Operation::DeleteVideo),
        ("GET", Some("content"), None) => Some(Operation::DownloadVideoContent),
        ("POST", Some("remix"), None) => Some(Operation::RemixVideo),
        _ => None,
    }
}

const fn content_operation(stream: bool) -> Operation {
    if stream {
        Operation::StreamGenerateContent
    } else {
        Operation::GenerateContent
    }
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
        if !rest.is_empty() {
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

/// Minimal peek over classify's one parsed body value. A type error elsewhere
/// must not flip the flag, and a non-bool `stream` / non-string `model` is
/// treated as absent.
fn peek_body(v: &serde_json::Value) -> (bool, Option<String>) {
    let stream = v
        .get("stream")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let model = v
        .get("model")
        .and_then(serde_json::Value::as_str)
        .or_else(|| v.get("session")?.get("model")?.as_str())
        .map(str::to_string);
    (stream, model)
}

fn json_bool(value: &serde_json::Value) -> bool {
    value
        .as_bool()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
        .unwrap_or(false)
}

/// Minimal body peek for the `"model"` field (tolerant, as above). Prefer
/// `ctx.body_model` — this is the fallback for contexts built without
/// [`classify`] (tests, direct pipeline entry).
pub(crate) fn peek_model(body: &Bytes) -> Option<String> {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("model").and_then(|m| m.as_str().map(str::to_string)))
}

/// Model id embedded in the path (gemini `models/{id}:verb`, `/v1/models/{id}`).
/// Only matches a `/models/{id}` segment — non-model paths return `None`.
pub(crate) fn path_model_id(path: &str) -> Option<String> {
    let (_, rest) = path.rsplit_once("/models/")?;
    if rest.is_empty() {
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
        (c.op.operation(), c.op.kind())
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
        assert_eq!(c.op.operation(), Operation::StreamGenerateContent);
    }

    #[test]
    fn body_stream_flag_selects_content_operation() {
        for path in ["/v1/chat/completions", "/v1/responses", "/v1/messages"] {
            let streaming = classify(
                &Method::POST,
                path,
                &HeaderMap::new(),
                &Bytes::from_static(b"{\"stream\":true}"),
            )
            .unwrap();
            assert_eq!(streaming.op.operation(), Operation::StreamGenerateContent);
            assert!(streaming.stream);

            let buffered = classify(
                &Method::POST,
                path,
                &HeaderMap::new(),
                &Bytes::from_static(b"{\"stream\":false}"),
            )
            .unwrap();
            assert_eq!(buffered.op.operation(), Operation::GenerateContent);
            assert!(!buffered.stream);
        }
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
    fn image_edit_path_classifies_as_openai_edit_image() {
        let body = Bytes::new();
        let c = classify(&Method::POST, "/v1/images/edits", &HeaderMap::new(), &body).unwrap();
        assert_eq!(
            op(&c),
            (Operation::EditImage, OperationKind::Provider(Prov::OpenAi))
        );
        assert!(!c.stream);
    }

    #[test]
    fn image_paths_honor_boolean_stream_flag() {
        for (path, operation) in [
            ("/v1/images/generations", Operation::CreateImage),
            ("/v1/images/edits", Operation::EditImage),
        ] {
            let c = classify(
                &Method::POST,
                path,
                &HeaderMap::new(),
                &Bytes::from_static(br#"{"model":"gpt-image","stream":true}"#),
            )
            .unwrap();
            assert_eq!(op(&c), (operation, OperationKind::Provider(Prov::OpenAi)));
            assert!(c.stream);
        }
    }

    #[test]
    fn image_edit_honors_multipart_normalized_stream_flag() {
        let c = classify(
            &Method::POST,
            "/v1/images/edits",
            &HeaderMap::new(),
            &Bytes::from_static(br#"{"model":"gpt-image","stream":"true"}"#),
        )
        .unwrap();
        assert_eq!(
            op(&c),
            (Operation::EditImage, OperationKind::Provider(Prov::OpenAi))
        );
        assert!(c.stream);
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
        // Multi-segment model ids (e.g. OpenRouter `vendor/model`) are valid
        // GetModel — the slash rejection was dropped for scoped model aliases.
        let c = classify(&Method::GET, "/v1/models/a/b", &HeaderMap::new(), &body).unwrap();
        assert_eq!(
            op(&c),
            (Operation::GetModel, OperationKind::Provider(Prov::OpenAi))
        );
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
    fn compact_path_classifies_as_openai_compact() {
        let body = Bytes::new();
        let c = classify(
            &Method::POST,
            "/v1/responses/compact",
            &HeaderMap::new(),
            &body,
        )
        .unwrap();
        assert_eq!(
            op(&c),
            (
                Operation::CompactContent,
                OperationKind::Provider(Prov::OpenAi)
            )
        );
        assert!(!c.stream);
    }

    #[test]
    fn rerank_path_classifies_as_buffered_openai_provider_operation() {
        let body = Bytes::from_static(
            br#"{"model":"reranker","query":"test","documents":["a","b"],"top_n":1,"stream":true}"#,
        );
        let c = classify(&Method::POST, "/v1/rerank", &HeaderMap::new(), &body).unwrap();
        assert_eq!(
            op(&c),
            (Operation::Rerank, OperationKind::Provider(Prov::OpenAi))
        );
        assert!(!c.stream, "the upstream rerank API has no SSE transport");
        assert_eq!(c.body_model.as_deref(), Some("reranker"));
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
