use base64::Engine as _;
use gproxy_channel_api::{ChannelError, ResourceCtx, ResourceMutation, UsageCtx};
use gproxy_protocol::gemini::VeoOperation;

pub(super) fn settlement_ready(ctx: UsageCtx<'_>) -> Result<bool, ChannelError> {
    let operation: VeoOperation = serde_json::from_slice(ctx.response_body).map_err(json_error)?;
    Ok(operation.done == Some(true) && operation.error.is_none())
}

pub(super) fn mutations(ctx: ResourceCtx<'_>) -> Result<Vec<ResourceMutation>, ChannelError> {
    if !matches!(
        ctx.key.operation(),
        gproxy_protocol::Operation::CreateVideo | gproxy_protocol::Operation::RetrieveVideo
    ) {
        return Ok(Vec::new());
    }
    let operation: VeoOperation = serde_json::from_slice(ctx.response_body).map_err(json_error)?;
    let name = operation
        .name
        .as_deref()
        .ok_or_else(|| observe("Vertex video operation has no name"))?;
    let id = encode_operation(name);
    if ctx
        .request_resource
        .is_some_and(|(_, request_id)| request_id != id)
    {
        return Err(observe("Vertex video operation differs from request"));
    }
    Ok(vec![ResourceMutation::Save {
        kind: "video",
        id,
        summary: serde_json::to_value(operation).map_err(json_error)?,
    }])
}

pub(super) fn request_operation(path: &str) -> Result<String, ChannelError> {
    let id = path
        .rsplit('/')
        .next()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| ChannelError::Prepare("Vertex video path has no operation id".into()))?;
    decode_operation(id)
}

pub(super) fn operation_model(operation: &str) -> Result<&str, ChannelError> {
    operation
        .split_once("/models/")
        .and_then(|(_, rest)| rest.split_once("/operations/"))
        .map(|(model, _)| model)
        .filter(|model| !model.is_empty())
        .ok_or_else(|| ChannelError::Prepare("Vertex operation has no model id".into()))
}

pub(super) fn encode_operation(operation: &str) -> String {
    format!(
        "gpx_{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(operation)
    )
}

fn decode_operation(id: &str) -> Result<String, ChannelError> {
    let encoded = id
        .strip_prefix("gpx_")
        .ok_or_else(|| ChannelError::Prepare("Vertex operation id is not a proxy alias".into()))?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|error| ChannelError::Prepare(format!("invalid Vertex operation id: {error}")))?;
    String::from_utf8(bytes)
        .map_err(|error| ChannelError::Prepare(format!("invalid Vertex operation id: {error}")))
}

fn json_error(error: serde_json::Error) -> ChannelError {
    ChannelError::Observe(format!("Vertex video response JSON: {error}"))
}

fn observe(message: &str) -> ChannelError {
    ChannelError::Observe(message.into())
}
