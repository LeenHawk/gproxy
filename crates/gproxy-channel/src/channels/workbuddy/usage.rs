//! Per-call metering.
//!
//! The chat surface is OpenAI Chat Completions verbatim, so the reading is
//! `shared::openai_wire`'s — the same one `openai`, `claudeapi` and `aistudio`
//! take of the identical wire. Images go through the channel's own unwrapping
//! first, so what reaches the extractor is an OpenAI image reply; when that
//! reply states no usage at all, the number of images it produced is still a
//! measurement and is recorded on its own (v3 `workbuddy/usage.rs`).

use super::WorkBuddy;
use crate::channel::{
    ChannelError, NormalizedUsage, UsageCompleteness, UsageContext, UsageExtractor, UsageObserver,
    UsageStream, UsageStreamContext,
};
use crate::channels::shared::openai_wire;
use gproxy_protocol::Operation;
use rust_decimal::Decimal;
use serde_json::Value;

/// How many images a reply carried, when it reported nothing else.
fn image_outputs(body: &[u8]) -> Option<NormalizedUsage> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let count = value.get("data").and_then(Value::as_array)?.len();
    if count == 0 {
        return None;
    }
    let mut usage = NormalizedUsage {
        completeness: UsageCompleteness::Complete,
        ..NormalizedUsage::default()
    };
    usage
        .metrics
        .insert("image_outputs".into(), Decimal::from(count));
    Some(usage)
}

impl UsageExtractor for WorkBuddy {
    fn extract(&self, context: UsageContext<'_>) -> Result<Option<NormalizedUsage>, ChannelError> {
        if !context.response.status.is_success() {
            return Ok(None);
        }
        if let Some(usage) = openai_wire::extract(&context) {
            return Ok(Some(usage));
        }
        if matches!(
            context.operation.operation,
            Operation::CreateImage | Operation::EditImage
        ) {
            return Ok(image_outputs(context.response.body));
        }
        Ok(None)
    }
}

impl UsageStream for WorkBuddy {
    fn start(
        &self,
        context: UsageStreamContext<'_>,
    ) -> Result<Box<dyn UsageObserver>, ChannelError> {
        openai_wire::observer(context.operation.operation, context.operation.dialect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_image_reply_that_states_no_tokens_still_states_how_many_images() {
        let body = json!({"data": [{"b64_json": "QUJD"}, {"b64_json": "REVG"}]}).to_string();
        let usage = image_outputs(body.as_bytes()).unwrap();
        assert_eq!(usage.metrics["image_outputs"], Decimal::from(2));
        assert!(image_outputs(json!({"data": []}).to_string().as_bytes()).is_none());
        assert!(image_outputs(b"not json").is_none());
    }
}
