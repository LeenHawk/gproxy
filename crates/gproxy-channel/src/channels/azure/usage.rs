//! Per-call metering. Azure returns the hosted vendor's own usage block, so
//! the counts are read with that vendor's field names.

use crate::channel::{ChannelError, NormalizedUsage, UsageContext, UsageExtractor};
use crate::channels::shared::vendor_usage;
use gproxy_protocol::Operation;

pub(super) struct AzureUsage;

impl UsageExtractor for AzureUsage {
    fn extract(&self, context: UsageContext<'_>) -> Result<Option<NormalizedUsage>, ChannelError> {
        if !context.response.status.is_success() || !is_metered(context.operation.operation) {
            return Ok(None);
        }
        vendor_usage::from_body(context.operation.dialect, context.response.body)
    }
}

/// Operations that consume model tokens. Counting tokens and listing models
/// report nothing to bill even when their answers contain numbers.
fn is_metered(operation: Operation) -> bool {
    matches!(
        operation,
        Operation::GenerateContent
            | Operation::StreamGenerateContent
            | Operation::CompactContent
            | Operation::CreateEmbedding
            | Operation::CreateImage
            | Operation::EditImage
    )
}
