use super::*;
use gproxy_protocol::connection::Bytes;

pub(super) fn list(bytes: &Bytes) -> Result<Vec<ModelInfo>> {
    let body: gproxy_protocol::claude::models::ListModelsResponseBody = parse(bytes)?;
    body.data.into_iter().map(one).collect()
}

pub(super) fn one(native: gproxy_protocol::claude::models::ModelInfo) -> Result<ModelInfo> {
    Ok(ModelInfo {
        provider_metadata: metadata(Vendor::Claude, &native)?,
        model: Model {
            id: native.id,
            dialect: Dialect::Claude,
        },
        display_name: Some(native.display_name),
        description: None,
        context_window: limit(native.max_input_tokens)?,
        max_output_tokens: limit(native.max_tokens)?,
    })
}
