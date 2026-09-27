//! Gemini `usageMetadata`, buffered and streamed.
//!
//! `promptTokenCount` is the whole input including the cached read, which
//! `cachedContentTokenCount` names separately, so the cached part comes back
//! out of the normalized input. `candidatesTokenCount` excludes thinking,
//! which Google reports as `thoughtsTokenCount`; the normalized output
//! includes it, so the two are added and thinking is recorded as the
//! reasoning subset. The image and audio parts of the candidates, from
//! `candidatesTokensDetails`, are recorded as metrics that are subsets of the
//! output and flagged as such. `toolUsePromptTokenCount` counts the results
//! of tools the model called, which the prompt count does not include.
//!
//! A stream carries the same object on its records, framed either as SSE or
//! as one incrementally delivered JSON array. Later records restate the
//! running totals rather than adding to them, so the last reading wins.

use serde::Deserialize;
use serde_json::Value;

use super::common::{self, count};
use super::types::{NormalizedUsage, UsageCompleteness};

/// Tokens of one modality inside a `*TokensDetails` array.
fn modality(details: Option<&Value>, name: &str) -> u64 {
    details
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|entry| entry.get("modality").and_then(Value::as_str) == Some(name))
        .filter_map(|entry| count(entry, "tokenCount"))
        .sum()
}

/// A `usageMetadata` object, or `None` when it reports no count at all.
///
/// Gemini leaves out a count that is zero, so in a settled reply a missing
/// output count is no output: a blocked prompt reports no candidates. Early
/// in a stream the same absence only means the output is not counted yet;
/// an unsettled reading leaves it unknown, for the estimator to fill, rather
/// than inventing a zero that would understate a cut stream.
pub(super) fn from_metadata(metadata: &Value, settled: bool) -> Option<NormalizedUsage> {
    let prompt = count(metadata, "promptTokenCount");
    let candidates = count(metadata, "candidatesTokenCount");
    let thoughts = count(metadata, "thoughtsTokenCount");
    if prompt.is_none() && candidates.is_none() && thoughts.is_none() {
        return None;
    }
    let cached = count(metadata, "cachedContentTokenCount")
        .unwrap_or_default()
        .min(prompt.unwrap_or(u64::MAX));
    let mut usage = NormalizedUsage {
        completeness: UsageCompleteness::Complete,
        ..NormalizedUsage::default()
    };
    usage.tokens.input_tokens = prompt.map(|prompt| prompt - cached);
    usage.tokens.cached_input_tokens = Some(cached);
    usage.tokens.output_tokens = if candidates.is_none() && thoughts.is_none() && !settled {
        None
    } else {
        Some(
            candidates
                .unwrap_or_default()
                .saturating_add(thoughts.unwrap_or_default()),
        )
    };
    usage.tokens.reasoning_tokens = thoughts;
    let details = metadata.get("candidatesTokensDetails");
    common::metric(
        &mut usage,
        "image_output_tokens",
        modality(details, "IMAGE"),
    );
    common::metric(
        &mut usage,
        "audio_output_tokens",
        modality(details, "AUDIO"),
    );
    common::flag_modalities(&mut usage);
    common::metric(
        &mut usage,
        "tool_use_prompt_tokens",
        count(metadata, "toolUsePromptTokenCount").unwrap_or_default(),
    );
    common::service_tier(
        &mut usage,
        metadata
            .get("serviceTier")
            .and_then(Value::as_str)
            .map(str::to_owned),
    );
    Some(usage)
}

/// A reply or stream record, reduced to its usage.
#[derive(Deserialize)]
struct Reply {
    #[serde(
        rename = "usageMetadata",
        alias = "usage_metadata",
        default,
        deserialize_with = "common::object"
    )]
    usage: Option<Value>,
}

/// Usage from a buffered `generateContent` reply.
pub(super) fn whole(body: &[u8]) -> Option<NormalizedUsage> {
    let reply: Reply = serde_json::from_slice(body).ok()?;
    from_metadata(&reply.usage?, true)
}

/// An embedding reply, which reports only the input side.
pub(super) fn embedding(body: &[u8]) -> Option<NormalizedUsage> {
    let reply: Reply = serde_json::from_slice(body).ok()?;
    let prompt = count(&reply.usage?, "promptTokenCount")?;
    let mut usage = NormalizedUsage {
        completeness: UsageCompleteness::Complete,
        ..NormalizedUsage::default()
    };
    usage.tokens.input_tokens = Some(prompt);
    Some(usage)
}

/// Watches a `streamGenerateContent` response.
#[derive(Default)]
pub(super) struct Stream {
    /// The last `usageMetadata` seen. It is kept raw because whether a
    /// missing output count is zero depends on how the stream ends.
    metadata: Option<Value>,
}

impl Stream {
    /// One SSE record. Most carry only content, so a record that does not
    /// mention usage is not parsed.
    pub(super) fn event(&mut self, data: &str) {
        if !data.contains("usageMetadata") && !data.contains("usage_metadata") {
            return;
        }
        let Ok(reply) = serde_json::from_str::<Reply>(data) else {
            return;
        };
        if let Some(metadata) = reply.usage {
            self.keep(metadata);
        }
    }

    fn keep(&mut self, metadata: Value) {
        if from_metadata(&metadata, true).is_some() {
            self.metadata = Some(metadata);
        }
    }

    /// One element of a JSON-array stream, which its decoder has already
    /// parsed whole.
    pub(super) fn record(&mut self, record: &Value) {
        let metadata = record
            .get("usageMetadata")
            .or_else(|| record.get("usage_metadata"));
        if let Some(metadata) = metadata {
            self.keep(metadata.clone());
        }
    }

    /// The reading so far; `settled` once the stream ended on its own.
    pub(super) fn reading(&self, settled: bool) -> Option<NormalizedUsage> {
        from_metadata(self.metadata.as_ref()?, settled)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{chunked, feed_chunks, feed_sse};
    use super::super::{UsageStreamEnd, whole as read_whole};
    use super::*;
    use crate::connection::StreamFraming;
    use crate::usage::UsageTransport;
    use crate::{Dialect, Operation};
    use serde_json::json;

    #[test]
    fn a_reply_separates_cache_thoughts_and_media() {
        let body = json!({"candidates": [{"content": {"parts": [{"text": "usageMetadata"}]}}],
            "usageMetadata": {"promptTokenCount": 100, "cachedContentTokenCount": 30,
            "candidatesTokenCount": 50, "thoughtsTokenCount": 20, "toolUsePromptTokenCount": 4,
            "candidatesTokensDetails": [{"modality": "TEXT", "tokenCount": 10},
                {"modality": "IMAGE", "tokenCount": 40}],
            "serviceTier": "flex"}});
        let usage = read_whole(
            Operation::GenerateContent,
            Dialect::Gemini,
            body.to_string().as_bytes(),
        )
        .unwrap();
        assert_eq!(usage.tokens.input_tokens, Some(70));
        assert_eq!(usage.tokens.cached_input_tokens, Some(30));
        assert_eq!(usage.tokens.output_tokens, Some(70), "thinking is output");
        assert_eq!(usage.tokens.reasoning_tokens, Some(20));
        assert_eq!(usage.metrics["image_output_tokens"], 40.into());
        assert_eq!(usage.metrics["tool_use_prompt_tokens"], 4.into());
        assert_eq!(usage.dimensions[common::MODALITIES_IN_TOTALS], "true");
        assert_eq!(usage.actual_service_tier.as_deref(), Some("flex"));

        let blocked = read_whole(
            Operation::GenerateContent,
            Dialect::Gemini,
            br#"{"promptFeedback":{"blockReason":"SAFETY"},"usageMetadata":{"promptTokenCount":9}}"#,
        )
        .unwrap();
        assert_eq!(blocked.tokens.input_tokens, Some(9));
        assert_eq!(blocked.tokens.output_tokens, Some(0));
        assert!(
            read_whole(
                Operation::GenerateContent,
                Dialect::Gemini,
                b"{\"usageMetadata\":{}}"
            )
            .is_none()
        );
    }

    const RECORDS: [&str; 3] = [
        r#"{"candidates":[{"content":{"parts":[{"text":"a"}]}}],"usageMetadata":{"promptTokenCount":10}}"#,
        r#"{"candidates":[{"content":{"parts":[{"text":"b"}]}}]}"#,
        r#"{"candidates":[{"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":5,"thoughtsTokenCount":2}}"#,
    ];

    #[test]
    fn an_sse_stream_takes_the_last_reading_in_any_chunking() {
        let wire: String = RECORDS
            .iter()
            .map(|r| format!("data: {r}\r\n\r\n"))
            .collect();
        for split in [1, 4, 33, usize::MAX] {
            let usage = feed_sse(
                Operation::StreamGenerateContent,
                Dialect::Gemini,
                &chunked(&wire, split),
                UsageStreamEnd::Complete,
            )
            .unwrap();
            assert_eq!(usage.tokens.output_tokens, Some(7), "split {split}");
            assert_eq!(usage.completeness, UsageCompleteness::Complete);
        }
        let cut: String = RECORDS[..2]
            .iter()
            .map(|r| format!("data: {r}\n\n"))
            .collect();
        let partial = feed_sse(
            Operation::StreamGenerateContent,
            Dialect::Gemini,
            &chunked(&cut, 6),
            UsageStreamEnd::Interrupted,
        )
        .unwrap();
        assert_eq!(partial.tokens.input_tokens, Some(10));
        assert_eq!(
            partial.tokens.output_tokens, None,
            "a cut stream's output is unknown, not zero"
        );
        assert_eq!(partial.completeness, UsageCompleteness::Partial);
    }

    #[test]
    fn a_json_array_stream_is_read_element_by_element() {
        let wire = format!("[{}]", RECORDS.join(",\r\n"));
        for split in [1, 5, 64, usize::MAX] {
            let usage = feed_chunks(
                Operation::StreamGenerateContent,
                Dialect::Gemini,
                UsageTransport::Http {
                    framing: Some(StreamFraming::JsonArray),
                },
                &chunked(&wire, split),
                UsageStreamEnd::Complete,
            )
            .unwrap();
            assert_eq!(usage.tokens.output_tokens, Some(7), "split {split}");
        }
        assert!(
            feed_chunks(
                Operation::StreamGenerateContent,
                Dialect::Gemini,
                UsageTransport::Http { framing: None },
                &chunked("{not an array", 3),
                UsageStreamEnd::Complete,
            )
            .is_none()
        );
    }
}
