use super::*;
use gproxy_protocol::connection::Bytes;

pub(super) fn list(bytes: &Bytes) -> Result<Vec<ModelInfo>> {
    let body: gproxy_protocol::gemini::models::ListModelsResponseBody = parse(bytes)?;
    body.models
        .unwrap_or_default()
        .into_iter()
        .map(one)
        .collect()
}

pub(super) fn one(native: gproxy_protocol::gemini::models::Model) -> Result<ModelInfo> {
    // Gemini names the resource `models/{id}`; requests take the bare id.
    let id = native
        .name
        .strip_prefix("models/")
        .unwrap_or(&native.name)
        .to_owned();
    Ok(ModelInfo {
        provider_metadata: metadata(Vendor::Gemini, &native)?,
        model: Model {
            id,
            dialect: Dialect::Gemini,
        },
        display_name: native.display_name,
        description: native.description,
        context_window: limit(native.input_token_limit)?,
        max_output_tokens: limit(native.output_token_limit)?,
    })
}
