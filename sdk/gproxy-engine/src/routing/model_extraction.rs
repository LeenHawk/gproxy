use crate::routing::classify::extract_model_from_uri_path;
use crate::routing::error::RoutingError;
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

/// Extracts a model identifier from a request path or JSON body.
pub fn extract_model(
    uri_path: &str,
    body: Option<&[u8]>,
    operation: OperationFamily,
    protocol: ProtocolKind,
) -> Option<String> {
    if matches!(
        operation,
        OperationFamily::ModelList
            | OperationFamily::FileUpload
            | OperationFamily::FileList
            | OperationFamily::FileGet
            | OperationFamily::FileContent
            | OperationFamily::FileDelete
    ) {
        return None;
    }

    match model_source(operation, protocol) {
        ModelSource::UriPath => extract_model_from_uri_path(uri_path),
        ModelSource::Body(pointer) => extract_model_from_body(body, pointer, operation, protocol),
        ModelSource::BodyOrUriPath(pointer) => extract_model_from_uri_path(uri_path)
            .or_else(|| extract_model_from_body(body, pointer, operation, protocol)),
    }
}

fn extract_model_from_body(
    body: Option<&[u8]>,
    pointer: &str,
    operation: OperationFamily,
    protocol: ProtocolKind,
) -> Option<String> {
    let bytes = body?;
    if bytes.is_empty() {
        return None;
    }
    let json: serde_json::Value = match serde_json::from_slice(bytes) {
        Ok(json) => json,
        Err(error) => {
            let routing_error = RoutingError::JsonDecode {
                kind: "request",
                operation,
                protocol,
                message: error.to_string(),
            };
            tracing::debug!(error = %routing_error, "failed to decode request body while extracting model");
            return None;
        }
    };
    let value = json.pointer(pointer)?;
    value.as_str().map(ToOwned::to_owned)
}

enum ModelSource {
    UriPath,
    Body(&'static str),
    BodyOrUriPath(&'static str),
}

fn model_source(operation: OperationFamily, protocol: ProtocolKind) -> ModelSource {
    match (operation, protocol) {
        (OperationFamily::ModelList, _) => ModelSource::Body("/model"),
        (OperationFamily::ModelGet, _) => ModelSource::UriPath,
        (
            OperationFamily::GenerateContent | OperationFamily::StreamGenerateContent,
            ProtocolKind::Gemini | ProtocolKind::GeminiNDJson,
        )
        | (OperationFamily::Embedding, ProtocolKind::Gemini | ProtocolKind::GeminiNDJson) => {
            ModelSource::UriPath
        }
        (OperationFamily::CountToken, ProtocolKind::Gemini | ProtocolKind::GeminiNDJson) => {
            ModelSource::BodyOrUriPath("/generate_content_request/model")
        }
        (OperationFamily::GeminiLive, ProtocolKind::Gemini) => ModelSource::Body("/setup/model"),
        _ => ModelSource::Body("/model"),
    }
}
