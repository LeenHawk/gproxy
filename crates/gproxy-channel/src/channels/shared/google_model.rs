//! Vertex publisher identities, following googleapis/python-genai's t_model:
//! `publisher/id` becomes `publishers/publisher/models/id`; the publisher is
//! moved into the resource hierarchy, never discarded. Bare ids mean Google
//! unless the caller explicitly selects the Anthropic wire.

use crate::channel::ChannelError;

pub(crate) fn from_path(path: &str) -> Result<Option<String>, ChannelError> {
    let tail = path
        .split_once("/models/")
        .map_or(path.trim_start_matches('/'), |(_, id)| id);
    let raw = tail.split(':').next().unwrap_or(tail);
    let model = percent_encoding::percent_decode_str(raw)
        .decode_utf8()
        .map_err(|error| ChannelError::InvalidConfig(error.to_string()))?;
    Ok((!model.is_empty()).then(|| model.into_owned()))
}

#[cfg(feature = "vertex")]
pub(crate) fn publisher(model: &str) -> Option<&str> {
    let model = model.strip_prefix("models/").unwrap_or(model);
    let model = if model.starts_with("projects/") {
        model.split_once("/publishers/")?.1
    } else {
        model.strip_prefix("publishers/").unwrap_or(model)
    };
    model.split_once('/').map(|(publisher, _)| publisher)
}

pub(crate) fn resource(model: &str, default_publisher: &str) -> Result<String, ChannelError> {
    let model = model.strip_prefix("models/").unwrap_or(model);
    if model
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(ChannelError::InvalidConfig(format!(
            "`{model}` is not a Vertex model resource"
        )));
    }
    if model.starts_with("projects/") || model.starts_with("publishers/") {
        return Ok(model.to_owned());
    }
    Ok(match model.split_once('/') {
        Some((publisher, id)) => format!("publishers/{publisher}/models/{id}"),
        None => format!("publishers/{default_publisher}/models/{model}"),
    })
}

/// URL escaping is separate from identity: unusual characters are data, not
/// a reason to reject or rename a model. Keep resource separators and `@`.
pub(crate) fn resource_path(model: &str, default_publisher: &str) -> Result<String, ChannelError> {
    const ESCAPE: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'_')
        .remove(b'.')
        .remove(b'~')
        .remove(b'/')
        .remove(b'@');
    let resource = resource(model, default_publisher)?;
    Ok(percent_encoding::utf8_percent_encode(&resource, ESCAPE).to_string())
}

/// Anthropic's countTokens body names the model within its publisher.
#[cfg(feature = "vertex")]
pub(crate) fn anthropic_id(model: &str) -> Result<String, ChannelError> {
    let resource = resource(model, "anthropic")?;
    let resource = resource
        .strip_prefix("publishers/")
        .or_else(|| resource.split_once("/publishers/").map(|(_, rest)| rest))
        .ok_or_else(|| {
            ChannelError::InvalidConfig("expected an Anthropic publisher model".into())
        })?;
    resource
        .strip_prefix("anthropic/models/")
        .map(str::to_owned)
        .ok_or_else(|| ChannelError::InvalidConfig("expected an Anthropic publisher model".into()))
}
