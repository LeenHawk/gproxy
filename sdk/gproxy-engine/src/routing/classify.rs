use http::{HeaderMap, Method};
use serde::Deserialize;

use crate::routing::error::RoutingError;
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

/// Classification metadata derived from an incoming request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Classification {
    /// The normalized operation family inferred from the route and payload.
    pub operation: OperationFamily,
    /// The protocol kind inferred from the route shape and request metadata.
    pub protocol: ProtocolKind,
    /// Whether the classified operation is a streaming operation.
    pub is_stream: bool,
}

impl Classification {
    /// Creates a new classification with a derived `is_stream` flag.
    pub const fn new(operation: OperationFamily, protocol: ProtocolKind) -> Self {
        Self {
            operation,
            protocol,
            is_stream: operation.is_stream(),
        }
    }
}

/// Classifies a request using only HTTP metadata and an optional buffered body.
///
/// The `path` argument may be either a raw path or a `path?query` string.
pub fn classify_route(
    method: &Method,
    path: &str,
    headers: &HeaderMap,
    body: Option<&[u8]>,
) -> Result<Classification, RoutingError> {
    let (raw_path, query) = split_path_and_query(path);
    let normalized_path = normalize_path(raw_path);
    let body = body.unwrap_or_default();

    if *method == Method::GET {
        if normalized_path == "/models" {
            return Ok(Classification::new(
                OperationFamily::ModelList,
                classify_models_protocol(headers, query),
            ));
        }
        if is_model_get_path(&normalized_path) {
            return Ok(Classification::new(
                OperationFamily::ModelGet,
                classify_models_protocol(headers, query),
            ));
        }
        if normalized_path == "/files" {
            return Ok(Classification::new(
                OperationFamily::FileList,
                classify_models_protocol(headers, query),
            ));
        }
        if is_file_content_path(&normalized_path) {
            return Ok(Classification::new(
                OperationFamily::FileContent,
                classify_models_protocol(headers, query),
            ));
        }
        if is_file_get_path(&normalized_path) {
            return Ok(Classification::new(
                OperationFamily::FileGet,
                classify_models_protocol(headers, query),
            ));
        }
        return Err(RoutingError::Unsupported("unsupported GET path"));
    }

    if *method == Method::DELETE {
        if extract_file_id_from_normalized(&normalized_path).is_some() {
            return Ok(Classification::new(
                OperationFamily::FileDelete,
                classify_models_protocol(headers, query),
            ));
        }
        return Err(RoutingError::Unsupported("unsupported DELETE path"));
    }

    if *method != Method::POST {
        return Err(RoutingError::Unsupported("unsupported HTTP method"));
    }

    if normalized_path == "/files" {
        return Ok(Classification::new(
            OperationFamily::FileUpload,
            classify_models_protocol(headers, query),
        ));
    }

    if normalized_path == "/responses" {
        return Ok(Classification::new(
            stream_or_non_stream(body),
            ProtocolKind::OpenAiResponse,
        ));
    }
    if normalized_path == "/chat/completions" {
        return Ok(Classification::new(
            stream_or_non_stream(body),
            ProtocolKind::OpenAiChatCompletion,
        ));
    }
    if normalized_path == "/messages" {
        return Ok(Classification::new(
            stream_or_non_stream(body),
            ProtocolKind::Claude,
        ));
    }
    if normalized_path == "/responses/input_tokens"
        || normalized_path == "/responses/input_tokens/count"
    {
        return Ok(Classification::new(
            OperationFamily::CountToken,
            ProtocolKind::OpenAi,
        ));
    }
    if normalized_path == "/messages/count_tokens" || normalized_path == "/messages/count-tokens" {
        return Ok(Classification::new(
            OperationFamily::CountToken,
            ProtocolKind::Claude,
        ));
    }
    if normalized_path == "/responses/compact" {
        return Ok(Classification::new(
            OperationFamily::Compact,
            ProtocolKind::OpenAi,
        ));
    }
    if normalized_path == "/embeddings" {
        return Ok(Classification::new(
            OperationFamily::Embedding,
            ProtocolKind::OpenAi,
        ));
    }
    if normalized_path == "/images/generations" {
        let operation = if read_stream_flag(body) {
            OperationFamily::StreamCreateImage
        } else {
            OperationFamily::CreateImage
        };
        return Ok(Classification::new(operation, ProtocolKind::OpenAi));
    }
    if normalized_path == "/images/edits" {
        let operation = if read_stream_flag(body) {
            OperationFamily::StreamCreateImageEdit
        } else {
            OperationFamily::CreateImageEdit
        };
        return Ok(Classification::new(operation, ProtocolKind::OpenAi));
    }
    if let Some((operation, protocol)) = classify_gemini(&normalized_path, query) {
        return Ok(Classification::new(operation, protocol));
    }

    Err(RoutingError::Unsupported("unable to classify request"))
}

/// Normalizes a request path by removing version prefixes and duplicate slashes.
pub fn normalize_path(path: &str) -> String {
    let mut out = if path.starts_with('/') {
        path.trim().to_string()
    } else {
        format!("/{}", path.trim())
    };
    while out.contains("//") {
        out = out.replace("//", "/");
    }
    if out.len() > 1 && out.ends_with('/') {
        out.pop();
    }
    if let Some(scoped) = strip_scoped_provider_prefix(&out) {
        out = scoped;
    }
    for prefix in ["/v1", "/v1beta", "/v1beta1"] {
        if out == prefix {
            return "/".to_string();
        }
        let full_prefix = format!("{prefix}/");
        if let Some(rest) = out.strip_prefix(&full_prefix) {
            return format!("/{}", rest.trim_start_matches('/'));
        }
    }
    out
}

fn strip_scoped_provider_prefix(path: &str) -> Option<String> {
    let trimmed = path.trim_matches('/');
    let mut parts = trimmed.splitn(3, '/');
    let _provider = parts.next()?;
    let version = parts.next()?;
    if !matches!(version, "v1" | "v1beta" | "v1beta1") {
        return None;
    }
    let rest = parts.next().unwrap_or_default();
    if rest.is_empty() {
        Some(format!("/{version}"))
    } else {
        Some(format!("/{version}/{rest}"))
    }
}

/// Extracts the model component from a URI path.
pub fn extract_model_from_uri_path(path: &str) -> Option<String> {
    let normalized = normalize_path(path);
    let tail = normalized.strip_prefix("/models/")?;
    if tail.is_empty() {
        return None;
    }
    let model = match tail.split(':').next() {
        Some(model) => model,
        None => tail,
    };
    if model.is_empty() {
        return None;
    }
    Some(model.to_string())
}

fn split_path_and_query(path: &str) -> (&str, Option<&str>) {
    match path.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (path, None),
    }
}

fn classify_models_protocol(headers: &HeaderMap, query: Option<&str>) -> ProtocolKind {
    if headers.contains_key("anthropic-version")
        || headers.contains_key("anthropic-beta")
        || headers.contains_key("x-api-key")
        || query_has_key(query, "after_id")
        || query_has_key(query, "before_id")
        || query_has_key(query, "limit")
    {
        return ProtocolKind::Claude;
    }
    if headers.contains_key("x-goog-api-key")
        || query_has_key(query, "pageSize")
        || query_has_key(query, "pageToken")
        || query_has_key(query, "key")
    {
        return ProtocolKind::Gemini;
    }
    ProtocolKind::OpenAi
}

fn classify_gemini(path: &str, query: Option<&str>) -> Option<(OperationFamily, ProtocolKind)> {
    let tail = path.strip_prefix("/models/")?;
    let (_, action) = tail.rsplit_once(':')?;
    match action {
        "countTokens" => Some((OperationFamily::CountToken, ProtocolKind::Gemini)),
        "generateContent" => Some((OperationFamily::GenerateContent, ProtocolKind::Gemini)),
        "streamGenerateContent" => Some((
            OperationFamily::StreamGenerateContent,
            if query_has_value(query, "alt", "sse") {
                ProtocolKind::Gemini
            } else {
                ProtocolKind::GeminiNDJson
            },
        )),
        "embedContent" => Some((OperationFamily::Embedding, ProtocolKind::Gemini)),
        _ => None,
    }
}

fn is_model_get_path(path: &str) -> bool {
    let Some(tail) = path.strip_prefix("/models/") else {
        return false;
    };
    // Model ids may contain '/' (OpenRouter uses `vendor/model-name`),
    // but ':' still marks a Gemini action suffix (e.g. `:countTokens`)
    // which is a POST op, not ModelGet.
    !tail.is_empty() && !tail.contains(':')
}

fn is_file_get_path(path: &str) -> bool {
    extract_file_id_from_normalized(path).is_some()
}

fn is_file_content_path(path: &str) -> bool {
    let Some(tail) = path.strip_prefix("/files/") else {
        return false;
    };
    tail.ends_with("/content")
}

fn extract_file_id_from_normalized(path: &str) -> Option<&str> {
    let tail = path.strip_prefix("/files/")?;
    if tail.is_empty() || tail.contains('/') {
        return None;
    }
    Some(tail)
}

fn query_has_key(query: Option<&str>, key: &str) -> bool {
    query.is_some_and(|query| {
        query
            .split('&')
            .any(|pair| pair.split('=').next() == Some(key))
    })
}

fn query_has_value(query: Option<&str>, key: &str, value: &str) -> bool {
    query.is_some_and(|query| {
        query.split('&').any(|pair| {
            let mut parts = pair.splitn(2, '=');
            parts.next() == Some(key)
                && parts
                    .next()
                    .is_some_and(|part| part.eq_ignore_ascii_case(value))
        })
    })
}

fn stream_or_non_stream(body: &[u8]) -> OperationFamily {
    if read_stream_flag(body) {
        OperationFamily::StreamGenerateContent
    } else {
        OperationFamily::GenerateContent
    }
}

fn read_stream_flag(body: &[u8]) -> bool {
    #[derive(Deserialize)]
    struct StreamFlag {
        #[serde(default)]
        stream: Option<bool>,
    }

    if body.is_empty() {
        return false;
    }
    serde_json::from_slice::<StreamFlag>(body)
        .ok()
        .and_then(|value| value.stream)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use http::{HeaderMap, Method};

    use super::classify_route;
    use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

    #[test]
    fn classify_route_accepts_scoped_openai_chat_path() {
        let result = classify_route(
            &Method::POST,
            "/codex/v1/chat/completions",
            &HeaderMap::new(),
            Some(br#"{"stream":false}"#),
        )
        .expect("scoped route should classify");

        assert_eq!(result.operation, OperationFamily::GenerateContent);
        assert_eq!(result.protocol, ProtocolKind::OpenAiChatCompletion);
    }

    #[test]
    fn classify_route_accepts_scoped_openai_responses_path() {
        let result = classify_route(
            &Method::POST,
            "/codex/v1/responses",
            &HeaderMap::new(),
            Some(br#"{"stream":true}"#),
        )
        .expect("scoped route should classify");

        assert_eq!(result.operation, OperationFamily::StreamGenerateContent);
        assert_eq!(result.protocol, ProtocolKind::OpenAiResponse);
    }
}
