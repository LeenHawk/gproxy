//! What the upstream reports about a turn, and how the channel meters it.
//!
//! CodeWhisperer reports counts as `tokenUsage` inside the stream's
//! `metadataEvent`, under AWS's own names and with the input either given
//! whole or split into uncached, cache-read and cache-write parts (v3
//! `kiro/usage.rs`). The translator writes those counts into the
//! `response.completed` event it synthesizes, as a Responses `usage` object —
//! so metering itself is the ordinary OpenAI reading, done by
//! `shared::openai_wire` against the stream the client is actually handed.
//! There is no fourth usage reader here: the only Kiro-specific step is the
//! rename below, which belongs to response shaping.

use super::Kiro;
use crate::channel::{
    ChannelError, NormalizedUsage, UsageContext, UsageExtractor, UsageObserver, UsageStream,
    UsageStreamContext,
};
use crate::channels::shared::openai_wire;
use serde_json::{Map, Value};

/// `tokenUsage` as a Responses `usage` object, or `None` when the event
/// reported nothing. Absence is not a measured zero.
pub(super) fn response_usage(value: &Value) -> Option<Value> {
    let output = number(
        value,
        &[
            "outputTokens",
            "completionTokens",
            "totalOutputTokens",
            "output_tokens",
        ],
    );
    let cached = number(value, &["cacheReadInputTokens", "cache_read_input_tokens"]);
    let written = number(
        value,
        &[
            "cacheWriteInputTokens",
            "cacheCreationInputTokens",
            "cache_write_input_tokens",
        ],
    );
    let uncached = number(value, &["uncachedInputTokens", "uncached_input_tokens"]);
    let stated = number(
        value,
        &[
            "inputTokens",
            "promptTokens",
            "totalInputTokens",
            "input_tokens",
        ],
    );
    let total = number(value, &["totalTokens", "total_tokens"]);
    if [output, cached, written, uncached, stated, total]
        .iter()
        .all(Option::is_none)
    {
        return None;
    }
    let output = output.unwrap_or_default();
    // A stated input wins; otherwise the parts add up, and failing that the
    // total minus the output is what is left.
    let input = stated.unwrap_or_else(|| {
        let parts =
            uncached.unwrap_or_default() + cached.unwrap_or_default() + written.unwrap_or_default();
        if parts > 0 {
            parts
        } else {
            total.unwrap_or_default().saturating_sub(output)
        }
    });
    let mut usage = Map::new();
    usage.insert("input_tokens".into(), Value::from(input));
    usage.insert("output_tokens".into(), Value::from(output));
    usage.insert(
        "total_tokens".into(),
        Value::from(input.saturating_add(output)),
    );
    let mut details = Map::new();
    if let Some(cached) = cached.filter(|value| *value > 0) {
        details.insert("cached_tokens".into(), Value::from(cached));
    }
    if let Some(written) = written.filter(|value| *value > 0) {
        // `openai_wire` reads a compatible vendor's cache write from this
        // name and records it as cache creation, not as ordinary input.
        details.insert("cache_write_tokens".into(), Value::from(written));
    }
    if !details.is_empty() {
        usage.insert("input_tokens_details".into(), Value::Object(details));
    }
    Some(Value::Object(usage))
}

/// The first of `names` this object carries, as a number however it was
/// written.
fn number(value: &Value, names: &[&str]) -> Option<u64> {
    names.iter().find_map(|name| {
        let value = value.get(*name)?;
        value
            .as_u64()
            .or_else(|| value.as_i64().and_then(|value| u64::try_from(value).ok()))
            .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
    })
}

impl UsageExtractor for Kiro {
    fn extract(&self, context: UsageContext<'_>) -> Result<Option<NormalizedUsage>, ChannelError> {
        Ok(openai_wire::extract(&context))
    }
}

impl UsageStream for Kiro {
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
    fn a_split_input_adds_up_and_a_stated_one_wins() {
        let split = response_usage(&json!({
            "uncachedInputTokens": 10, "cacheReadInputTokens": 30,
            "cacheWriteInputTokens": 5, "outputTokens": 7,
        }))
        .unwrap();
        assert_eq!(split["input_tokens"], 45);
        assert_eq!(split["output_tokens"], 7);
        assert_eq!(split["total_tokens"], 52);
        assert_eq!(split["input_tokens_details"]["cached_tokens"], 30);
        assert_eq!(split["input_tokens_details"]["cache_write_tokens"], 5);

        let stated =
            response_usage(&json!({"inputTokens": "100", "outputTokens": 2, "totalTokens": 999}))
                .unwrap();
        assert_eq!(stated["input_tokens"], 100);
        assert!(stated.get("input_tokens_details").is_none());
    }

    #[test]
    fn a_total_minus_the_output_is_the_last_resort_and_nothing_stays_nothing() {
        let derived = response_usage(&json!({"totalTokens": 30, "outputTokens": 8})).unwrap();
        assert_eq!(derived["input_tokens"], 22);
        assert!(response_usage(&json!({})).is_none());
    }

    #[test]
    fn the_shared_openai_reading_takes_the_cache_write_out_of_the_input() {
        let usage = response_usage(&json!({
            "uncachedInputTokens": 10, "cacheReadInputTokens": 30,
            "cacheWriteInputTokens": 5, "outputTokens": 7,
        }))
        .unwrap();
        let normalized = openai_wire::from_usage(&usage).unwrap();
        assert_eq!(normalized.tokens.input_tokens, Some(10));
        assert_eq!(normalized.tokens.cached_input_tokens, Some(30));
        assert_eq!(normalized.tokens.cache_creation_30m_tokens, Some(5));
        assert_eq!(normalized.tokens.output_tokens, Some(7));
    }
}
