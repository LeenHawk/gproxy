//! Per-call metering. Express mode returns Gemini's own `usageMetadata`.

use crate::channel::{ChannelError, NormalizedUsage, UsageContext, UsageExtractor};
use crate::channels::shared::vendor_usage;
use gproxy_protocol::Operation;

pub(super) struct VertexExpressUsage;

impl UsageExtractor for VertexExpressUsage {
    fn extract(&self, context: UsageContext<'_>) -> Result<Option<NormalizedUsage>, ChannelError> {
        let metered = matches!(
            context.operation.operation,
            Operation::GenerateContent | Operation::StreamGenerateContent
        );
        if !context.response.status.is_success() || !metered {
            return Ok(None);
        }
        vendor_usage::from_body(context.operation.dialect, context.response.body)
    }
}
