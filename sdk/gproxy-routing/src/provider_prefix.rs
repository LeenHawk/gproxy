use crate::classify::extract_model_from_uri_path;
use crate::error::RoutingError;
use crate::{OperationFamily, ProtocolKind};

/// Splits a provider-prefixed model string into `(has_models_prefix, provider, model)`.
pub fn split_provider_prefixed_model(value: &str) -> Option<(bool, &str, &str)> {
    let (has_models_prefix, tail) = if let Some(rest) = value.strip_prefix("models/") {
        (true, rest)
    } else {
        (false, value)
    };
    let (provider, model) = tail.split_once('/')?;
    if provider.is_empty() || model.is_empty() {
        return None;
    }
    Some((has_models_prefix, provider, model))
}

/// Adds a provider prefix to a model string unless one is already present.
pub fn add_provider_prefix(value: &str, provider: &str) -> String {
    if provider.is_empty() || split_provider_prefixed_model(value).is_some() {
        return value.to_string();
    }
    if let Some(rest) = value.strip_prefix("models/") {
        return format!("models/{provider}/{rest}");
    }
    if value.is_empty() {
        provider.to_string()
    } else {
        format!("{provider}/{value}")
    }
}

/// Removes a provider prefix from a model field inside a JSON body.
pub fn strip_provider_from_body(
    operation: OperationFamily,
    protocol: ProtocolKind,
    body: &[u8],
) -> Option<(String, Vec<u8>)> {
    let pointers = body_model_pointers(operation, protocol);
    if pointers.is_empty() || body.is_empty() {
        return None;
    }
    let mut value: serde_json::Value = match serde_json::from_slice(body) {
        Ok(value) => value,
        Err(error) => {
            let routing_error = RoutingError::JsonDecode {
                kind: "request",
                operation,
                protocol,
                message: error.to_string(),
            };
            tracing::debug!(error = %routing_error, "failed to decode request body while stripping provider prefix");
            return None;
        }
    };
    let mut provider: Option<String> = None;

    for pointer in pointers {
        let Some(slot) = value.pointer_mut(pointer) else {
            continue;
        };
        let Some(raw) = slot.as_str() else {
            continue;
        };
        let Some((has_models, current_provider, model)) = split_provider_prefixed_model(raw) else {
            continue;
        };
        if let Some(existing_provider) = &provider {
            if existing_provider != current_provider {
                let routing_error = RoutingError::ProviderPrefix {
                    message: format!(
                        "inconsistent provider prefix: expected {existing_provider}, found {current_provider}"
                    ),
                };
                tracing::debug!(error = %routing_error, "rejecting body with inconsistent provider prefixes");
                return None;
            }
        } else {
            provider = Some(current_provider.to_string());
        }
        *slot = serde_json::Value::String(if has_models {
            format!("models/{model}")
        } else {
            model.to_string()
        });
    }

    let provider = provider?;
    match serde_json::to_vec(&value) {
        Ok(new_body) => Some((provider, new_body)),
        Err(error) => {
            let routing_error = RoutingError::ProviderPrefix {
                message: format!("failed to serialize stripped request body: {error}"),
            };
            tracing::debug!(error = %routing_error, "failed to serialize provider-stripped body");
            None
        }
    }
}

/// Removes a provider prefix from a model encoded in a URI path.
pub fn strip_provider_from_uri_path(path: &str) -> Option<(String, String)> {
    let model_in_path = extract_model_from_uri_path(path)?;
    let (has_models, provider, model) = split_provider_prefixed_model(&model_in_path)?;
    let provider = provider.to_string();
    let model = model.to_string();
    let old_segment = if has_models {
        format!("models/{model_in_path}")
    } else {
        model_in_path
    };
    let new_segment = if has_models {
        format!("models/{model}")
    } else {
        model
    };
    let new_path = path.replace(&old_segment, &new_segment);
    Some((provider, new_path))
}

fn body_model_pointers(
    operation: OperationFamily,
    protocol: ProtocolKind,
) -> &'static [&'static str] {
    match (operation, protocol) {
        (OperationFamily::ModelGet, ProtocolKind::Gemini | ProtocolKind::GeminiNDJson) => &[],
        (
            OperationFamily::GenerateContent | OperationFamily::StreamGenerateContent,
            ProtocolKind::Gemini | ProtocolKind::GeminiNDJson,
        ) => &[],
        (OperationFamily::Embedding, ProtocolKind::Gemini | ProtocolKind::GeminiNDJson) => &[],
        (OperationFamily::ModelGet, _) => &[],
        (OperationFamily::CountToken, ProtocolKind::Gemini | ProtocolKind::GeminiNDJson) => {
            &["/generate_content_request/model"]
        }
        (OperationFamily::GeminiLive, ProtocolKind::Gemini) => &["/setup/model"],
        _ => &["/model"],
    }
}
