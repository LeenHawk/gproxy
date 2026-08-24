mod compact;
mod converse;
mod video;

use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx, ResponseShapeCtx};
use gproxy_protocol::Operation;

pub(super) fn is_compact(body: &[u8]) -> bool {
    compact::is_request(body)
}

pub(super) fn request(ctx: &PrepareCtx<'_>, compact_request: bool) -> Result<Bytes, ChannelError> {
    match ctx.key.operation {
        Operation::ListModels | Operation::GetModel => Ok(ctx.body.clone()),
        Operation::CreateVideo => {
            video::request(ctx.body, ctx.upstream_model, ctx.provider_settings)
        }
        Operation::RetrieveVideo => Ok(Bytes::new()),
        Operation::GenerateContent | Operation::StreamGenerateContent if compact_request => {
            compact::request(ctx.body)
        }
        Operation::CountTokens => converse::request(ctx.body, true, false),
        Operation::GenerateContent | Operation::StreamGenerateContent => converse::request(
            ctx.body,
            false,
            ctx.key.operation == Operation::StreamGenerateContent,
        ),
        _ => Err(ChannelError::Prepare(
            "operation is unsupported by AWS Bedrock".into(),
        )),
    }
}

pub(super) fn response(ctx: ResponseShapeCtx<'_>) -> Result<Bytes, ChannelError> {
    if !ctx.status.is_success() {
        return Ok(ctx.body.clone());
    }
    match ctx.key.operation {
        Operation::ListModels => super::model::response(ctx.body, false),
        Operation::GetModel => super::model::response(ctx.body, true),
        Operation::CountTokens => count_response(ctx.body),
        Operation::GenerateContent | Operation::StreamGenerateContent => {
            if serde_json::from_slice::<serde_json::Value>(ctx.body)
                .ok()
                .is_some_and(|value| value.get("output").is_some())
            {
                converse::response(ctx.body)
            } else {
                Ok(ctx.body.clone())
            }
        }
        Operation::CreateVideo | Operation::RetrieveVideo => video::response(ctx.body),
        _ => Ok(ctx.body.clone()),
    }
}

fn count_response(body: &Bytes) -> Result<Bytes, ChannelError> {
    let response: gproxy_protocol::aws::CountTokensResponse = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Observe(format!("Bedrock CountTokens JSON: {error}")))?;
    let output = gproxy_protocol::claude::CountTokensResponseBody {
        input_tokens: response.input_tokens,
        context_management: None,
        rest: response.rest,
    };
    serde_json::to_vec(&output)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Observe(error.to_string()))
}
