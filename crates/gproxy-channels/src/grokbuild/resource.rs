use gproxy_channel_api::{ChannelError, ResourceCtx, ResourceMutation, UsageCtx};
use gproxy_protocol::Operation;
use serde_json::Value;

pub(super) fn settlement_ready(ctx: UsageCtx<'_>) -> Result<bool, ChannelError> {
    if ctx.key.operation != Operation::RetrieveVideo {
        return Ok(false);
    }
    let value = json(ctx.response_body)?;
    let ready = matches!(
        value.get("status").and_then(Value::as_str),
        Some("done" | "succeeded" | "success" | "completed")
    ) || value.get("status").is_none() && value.pointer("/video/url").is_some();
    Ok(ready && value.get("error").is_none_or(Value::is_null))
}

pub(super) fn mutations(ctx: ResourceCtx<'_>) -> Result<Vec<ResourceMutation>, ChannelError> {
    if !matches!(
        ctx.key.operation,
        Operation::CreateVideo
            | Operation::RetrieveVideo
            | Operation::EditVideo
            | Operation::ExtendVideo
    ) {
        return Ok(Vec::new());
    }
    let value = json(ctx.response_body)?;
    let id = value
        .get("id")
        .or_else(|| value.get("request_id"))
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| ChannelError::Observe("video id missing".into()))?;
    if ctx
        .request_resource
        .is_some_and(|(_, requested)| requested != id)
    {
        return Err(ChannelError::Observe("response video id mismatch".into()));
    }
    Ok(vec![ResourceMutation::Save {
        kind: "video",
        id: id.into(),
        summary: value,
    }])
}

fn json(body: &[u8]) -> Result<Value, ChannelError> {
    serde_json::from_slice(body).map_err(|error| ChannelError::Observe(error.to_string()))
}
