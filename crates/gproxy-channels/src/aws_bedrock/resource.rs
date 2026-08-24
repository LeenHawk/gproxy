use base64::Engine as _;
use gproxy_channel_api::{ChannelError, ResourceCtx, ResourceMutation, UsageCtx};
use gproxy_protocol::Operation;
use serde_json::Value;

pub(super) fn settlement_ready(ctx: UsageCtx<'_>) -> Result<bool, ChannelError> {
    let value: Value = serde_json::from_slice(ctx.response_body).map_err(json_error)?;
    Ok(
        value.get("status").and_then(Value::as_str) == Some("Completed")
            && value.get("failureMessage").is_none(),
    )
}

pub(super) fn mutations(ctx: ResourceCtx<'_>) -> Result<Vec<ResourceMutation>, ChannelError> {
    if !matches!(
        ctx.key.operation,
        Operation::CreateVideo | Operation::RetrieveVideo
    ) {
        return Ok(Vec::new());
    }
    let value: Value = serde_json::from_slice(ctx.response_body).map_err(json_error)?;
    let arn = value
        .get("invocationArn")
        .and_then(Value::as_str)
        .ok_or_else(|| observe("Bedrock video response has no invocationArn"))?;
    let id = encode_arn(arn);
    if ctx
        .request_resource
        .is_some_and(|(_, request_id)| request_id != id)
    {
        return Err(observe("Bedrock video invocation differs from request"));
    }
    Ok(vec![ResourceMutation::Save {
        kind: "video",
        id,
        summary: value,
    }])
}

pub(super) fn request_arn(path: &str) -> Result<String, ChannelError> {
    let id = path
        .strip_prefix("/v1/videos/")
        .filter(|id| !id.is_empty() && !id.contains('/'))
        .ok_or_else(|| ChannelError::Prepare("invalid Bedrock video path".into()))?;
    decode_arn(id)
}

pub(super) fn encode_arn(arn: &str) -> String {
    format!(
        "gpx_{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(arn)
    )
}

fn decode_arn(id: &str) -> Result<String, ChannelError> {
    let encoded = id
        .strip_prefix("gpx_")
        .ok_or_else(|| ChannelError::Prepare("Bedrock video id is not a proxy alias".into()))?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|error| ChannelError::Prepare(format!("invalid Bedrock video id: {error}")))?;
    String::from_utf8(bytes)
        .map_err(|error| ChannelError::Prepare(format!("invalid Bedrock video id: {error}")))
}

pub(super) fn output_url(value: &Value) -> Option<String> {
    let prefix = value
        .pointer("/outputDataConfig/s3OutputDataConfig/s3Uri")?
        .as_str()?
        .trim_end_matches('/');
    let arn = value.get("invocationArn")?.as_str()?;
    let job = arn.rsplit('/').next()?;
    Some(format!("{prefix}/{job}/output.mp4"))
}

fn json_error(error: serde_json::Error) -> ChannelError {
    ChannelError::Observe(format!("Bedrock video response JSON: {error}"))
}

fn observe(message: &str) -> ChannelError {
    ChannelError::Observe(message.into())
}
