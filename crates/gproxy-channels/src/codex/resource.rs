use gproxy_channel_api::{ChannelError, ResourceCtx, ResourceMutation};
use gproxy_protocol::Affinity;

pub(super) fn mutations(ctx: ResourceCtx<'_>) -> Result<Vec<ResourceMutation>, ChannelError> {
    let Affinity::Resource(kind) = ctx.key.operation().spec().affinity else {
        return Ok(Vec::new());
    };
    if ctx.key.operation() != gproxy_protocol::Operation::CreateRealtimeCall {
        return Ok(Vec::new());
    }
    let id = crate::shared::openai::realtime::call_id(ctx.response_headers)?;
    Ok(vec![ResourceMutation::Save {
        kind,
        summary: serde_json::json!({"id": id}),
        id,
    }])
}
