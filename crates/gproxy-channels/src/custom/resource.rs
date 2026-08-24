use gproxy_channel_api::{ChannelError, ResourceCtx, ResourceMutation, UsageCtx};
use gproxy_protocol::{Operation, OperationKind, WireFamily};

pub(super) fn settlement_ready(ctx: UsageCtx<'_>) -> Result<bool, ChannelError> {
    if ctx.key.operation == Operation::RetrieveVideo
        && ctx.key.kind == OperationKind::Family(WireFamily::OpenAi)
    {
        crate::shared::openai::resource::settlement_ready(ctx)
    } else {
        Ok(false)
    }
}

pub(super) fn mutations(ctx: ResourceCtx<'_>) -> Result<Vec<ResourceMutation>, ChannelError> {
    if ctx.key.kind == OperationKind::Family(WireFamily::OpenAi) {
        crate::shared::openai::resource::mutations(ctx)
    } else {
        Ok(Vec::new())
    }
}
