use gproxy_channel_api::{NormalizedUsage, UsageAttempt};
use serde_json::Value;

pub(in crate::shared::claude) fn attach(
    usage: &mut NormalizedUsage,
    wire: &Value,
    model: &str,
    refused: bool,
) {
    let iterations = wire.get("iterations").and_then(Value::as_array);
    if let Some(iterations) =
        iterations.filter(|items| items.iter().any(|item| item["type"] == "fallback_message"))
    {
        for (index, item) in iterations.iter().enumerate() {
            let Some(normalized) = super::from_usage(item) else {
                continue;
            };
            usage.attempts.push(UsageAttempt {
                estimated: false,
                model: item
                    .get("model")
                    .and_then(Value::as_str)
                    .unwrap_or(model)
                    .into(),
                billable: normalized.output_tokens > 0
                    || (!refused && index + 1 == iterations.len()),
                usage: Box::new(normalized),
                started_at_ms: None,
            });
        }
    } else if refused || !model.is_empty() {
        usage.attempts.push(UsageAttempt {
            estimated: false,
            model: model.into(),
            billable: !refused || usage.output_tokens > 0,
            usage: Box::new(usage.clone()),
            started_at_ms: None,
        });
    }
}
