use super::*;
use gproxy_protocol::connection::Bytes;

pub(super) fn list(bytes: &Bytes, dialect: Dialect) -> Result<Vec<ModelInfo>> {
    // `models` is the Codex projection of the same catalog; `data` is the list.
    let body: gproxy_protocol::openai::models::ListModelsResponseBody = parse(bytes)?;
    body.data.into_iter().map(|m| one(m, dialect)).collect()
}

pub(super) fn one(
    native: gproxy_protocol::openai::models::Model,
    dialect: Dialect,
) -> Result<ModelInfo> {
    Ok(ModelInfo {
        provider_metadata: metadata(Vendor::Openai, &native)?,
        model: Model {
            id: native.id,
            dialect,
        },
        display_name: native.display_name,
        description: native.description,
        context_window: native.context_window,
        max_output_tokens: native.max_output_tokens,
    })
}
