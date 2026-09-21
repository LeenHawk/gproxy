//! Usage, including the image envelope's own counters.

use super::DashScope;
use crate::channel::{
    ChannelError, NormalizedUsage, UsageCompleteness, UsageContext, UsageExtractor, UsageObserver,
    UsageStream, UsageStreamContext,
};
use crate::channels::shared::compatible::usage::{self, u64_at};
use serde_json::Value;

/// The image reply keeps DashScope's own usage object under its own name
/// (`image.rs`), counting whole images rather than OpenAI's token details.
fn enrich(root: &Value, _: &Value, into: &mut NormalizedUsage) {
    let Some(native) = root.get("dashscope_usage") else {
        return;
    };
    into.tokens.input_tokens = u64_at(native, "/input_tokens");
    into.tokens.output_tokens = u64_at(native, "/output_tokens");
    if let Some(count) = u64_at(native, "/image_count") {
        into.metrics.insert("image_outputs".into(), count.into());
    }
    if let Some(size) = native
        .get("size")
        .or_else(|| root.get("size"))
        .and_then(Value::as_str)
    {
        into.dimensions.insert("size".into(), size.to_owned());
    }
    into.completeness = UsageCompleteness::Complete;
}

impl UsageExtractor for DashScope {
    fn extract(&self, ctx: UsageContext<'_>) -> Result<Option<NormalizedUsage>, ChannelError> {
        if !ctx.response.status.is_success() {
            return Ok(None);
        }
        Ok(usage::from_body(
            ctx.operation.dialect,
            ctx.response.body,
            enrich,
        ))
    }
}

impl UsageStream for DashScope {
    fn start(
        &self,
        context: UsageStreamContext<'_>,
    ) -> Result<Box<dyn UsageObserver>, ChannelError> {
        Ok(usage::observer(
            context.operation.dialect,
            context.transport,
            enrich,
        ))
    }
}
