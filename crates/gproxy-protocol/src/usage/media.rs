//! The non-conversational operations: embeddings, images, audio
//! transcription and rerank.
//!
//! Each reads the dialect's usage object with the same field names as
//! generation, plus what the operation adds: the images a reply produced, the
//! seconds of audio a transcription was billed by, the search units of a
//! rerank.

use serde::Deserialize;
use serde_json::Value;

use super::common::{self, count, present};
use super::openai;
use super::types::{NormalizedUsage, UsageCompleteness};

/// A reply's usage object and nothing else.
#[derive(Deserialize)]
struct Reply {
    #[serde(default, deserialize_with = "common::object")]
    usage: Option<Value>,
}

// ------------------------------------------------------------- embeddings

/// An OpenAI embedding reply: `prompt_tokens` and `total_tokens`, input only.
pub(super) fn openai_embedding(body: &[u8]) -> Option<NormalizedUsage> {
    let reply: Reply = serde_json::from_slice(body).ok()?;
    openai::from_usage(&reply.usage?)
}

// ----------------------------------------------------------------- images

/// An image reply's usage. The token totals are the Responses shape; the
/// image tokens inside them become `image_input_tokens` and
/// `image_output_tokens`, flagged as subsets of the totals. A reply whose
/// output has no breakdown produced nothing but the image, so its whole
/// output is image tokens.
pub(super) fn image_usage(usage: &Value) -> Option<NormalizedUsage> {
    let mut normalized = openai::from_usage(usage)?;
    if !normalized.metrics.contains_key("image_output_tokens") {
        let output = normalized.tokens.output_tokens.unwrap_or_default();
        common::metric(&mut normalized, "image_output_tokens", output);
    }
    common::flag_modalities(&mut normalized);
    Some(normalized)
}

/// Record the image qualifiers that pricing rules condition on.
pub(super) fn qualify<const N: usize>(
    usage: &mut NormalizedUsage,
    qualifiers: [(&str, Option<String>); N],
) {
    for (name, value) in qualifiers {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            usage.dimensions.insert(name.into(), value);
        }
    }
}

#[derive(Deserialize)]
struct ImageReply {
    #[serde(default, deserialize_with = "common::object")]
    usage: Option<Value>,
    #[serde(default, deserialize_with = "common::length")]
    data: Option<usize>,
    #[serde(default, deserialize_with = "common::string")]
    size: Option<String>,
    #[serde(default, deserialize_with = "common::string")]
    quality: Option<String>,
    #[serde(default, deserialize_with = "common::string")]
    output_format: Option<String>,
    #[serde(default, deserialize_with = "common::string")]
    background: Option<String>,
}

/// A buffered image generation or edit. A reply that states no tokens — a
/// model priced per image does not — still states how many images it made,
/// and that count is a measurement of its own.
pub(super) fn image(body: &[u8]) -> Option<NormalizedUsage> {
    let reply: ImageReply = serde_json::from_slice(body).ok()?;
    let images = reply.data.unwrap_or_default();
    let mut usage = match reply.usage.as_ref().and_then(image_usage) {
        Some(usage) => usage,
        None if images > 0 => NormalizedUsage {
            completeness: UsageCompleteness::Complete,
            ..NormalizedUsage::default()
        },
        None => return None,
    };
    common::metric(&mut usage, "image_outputs", images as u64);
    qualify(
        &mut usage,
        [
            ("size", reply.size),
            ("quality", reply.quality),
            ("output_format", reply.output_format),
            ("background", reply.background),
        ],
    );
    Some(usage)
}

// ---------------------------------------------------------- transcription

/// A transcription's usage, which reports either tokens or `seconds` of
/// audio depending on the model; `"type": "duration"` marks the seconds-only
/// shape. The input's audio and text parts are subsets of its total.
fn transcription_usage(usage: &Value) -> Option<NormalizedUsage> {
    let tokens = usage.get("type").and_then(Value::as_str) != Some("duration")
        && present(usage, "input_tokens")
        && present(usage, "output_tokens");
    let seconds = usage.get("seconds").and_then(common::decimal);
    if !tokens && seconds.is_none() {
        return None;
    }
    let mut normalized = NormalizedUsage {
        completeness: UsageCompleteness::Complete,
        ..NormalizedUsage::default()
    };
    if tokens {
        normalized.tokens.input_tokens = count(usage, "input_tokens");
        normalized.tokens.output_tokens = count(usage, "output_tokens");
        if let Some(details) = usage.get("input_token_details") {
            for modality in ["audio", "text"] {
                common::metric(
                    &mut normalized,
                    &format!("{modality}_input_tokens"),
                    count(details, &format!("{modality}_tokens")).unwrap_or_default(),
                );
            }
            common::flag_modalities(&mut normalized);
        }
    }
    if let Some(seconds) = seconds {
        normalized.metrics.insert("audio_seconds".into(), seconds);
    }
    Some(normalized)
}

/// A buffered transcription or translation.
pub(super) fn transcription(body: &[u8]) -> Option<NormalizedUsage> {
    let reply: Reply = serde_json::from_slice(body).ok()?;
    transcription_usage(&reply.usage?)
}

#[derive(Deserialize)]
struct TranscriptEvent {
    #[serde(rename = "type", default, deserialize_with = "common::string")]
    kind: Option<String>,
    #[serde(default, deserialize_with = "common::object")]
    usage: Option<Value>,
}

/// Watches a streamed transcription, whose `transcript.text.done` event
/// carries the usage.
#[derive(Default)]
pub(super) struct TranscriptionStream {
    usage: Option<NormalizedUsage>,
}

impl TranscriptionStream {
    const DONE: &str = "transcript.text.done";

    pub(super) fn event(&mut self, name: Option<&str>, data: &str) -> bool {
        if name.is_some_and(|name| name != Self::DONE) || !data.contains(Self::DONE) {
            return false;
        }
        let Ok(event) = serde_json::from_str::<TranscriptEvent>(data) else {
            return false;
        };
        if event.kind.as_deref() != Some(Self::DONE) {
            return false;
        }
        let Some(usage) = event.usage.as_ref().and_then(transcription_usage) else {
            return false;
        };
        self.usage = Some(usage);
        true
    }

    pub(super) fn snapshot(&self) -> Option<NormalizedUsage> {
        self.usage.clone()
    }
}

// ----------------------------------------------------------------- rerank

/// A rerank reply. Rerank consumes input only: `prompt_tokens` where the
/// upstream splits it out, else `total_tokens`. A provider that bills by
/// search unit reports `search_units`, recorded as a metric of that name.
pub(super) fn rerank(body: &[u8]) -> Option<NormalizedUsage> {
    let reply: Reply = serde_json::from_slice(body).ok()?;
    let usage = reply.usage?;
    let input = count(&usage, "prompt_tokens").or_else(|| count(&usage, "total_tokens"));
    let units = count(&usage, "search_units");
    if input.is_none() && units.is_none() {
        return None;
    }
    let mut normalized = NormalizedUsage {
        completeness: UsageCompleteness::Complete,
        ..NormalizedUsage::default()
    };
    normalized.tokens.input_tokens = input;
    common::metric(&mut normalized, "search_units", units.unwrap_or_default());
    Some(normalized)
}

#[cfg(test)]
mod tests {
    use super::super::tests::{chunked, feed_sse};
    use super::super::{UsageStreamEnd, whole};
    use super::*;
    use crate::{Dialect, Operation};
    use serde_json::json;

    fn read(operation: Operation, dialect: Dialect, body: &Value) -> Option<NormalizedUsage> {
        whole(operation, dialect, body.to_string().as_bytes())
    }

    #[test]
    fn embeddings_are_input_only() {
        let openai = read(
            Operation::CreateEmbedding,
            Dialect::OpenAi,
            &json!({"object": "list", "data": [{"embedding": [0.1]}],
                "usage": {"prompt_tokens": 8, "total_tokens": 8}}),
        )
        .unwrap();
        assert_eq!(openai.tokens.input_tokens, Some(8));
        assert_eq!(openai.tokens.output_tokens, Some(0));
        let gemini = read(
            Operation::BatchCreateEmbedding,
            Dialect::Gemini,
            &json!({"embeddings": [], "usageMetadata": {"promptTokenCount": 6}}),
        )
        .unwrap();
        assert_eq!(gemini.tokens.input_tokens, Some(6));
        assert_eq!(gemini.tokens.output_tokens, None);
        assert!(
            read(
                Operation::CreateEmbedding,
                Dialect::OpenAi,
                &json!({"data": []})
            )
            .is_none()
        );
    }

    #[test]
    fn an_image_reply_counts_images_and_image_tokens() {
        let usage = read(
            Operation::CreateImage,
            Dialect::OpenAi,
            &json!({"data": [{"b64_json": "x"}, {"b64_json": "y"}], "quality": "high", "size": "1024x1024",
                "usage": {"input_tokens": 20, "output_tokens": 8, "total_tokens": 28,
                    "input_tokens_details": {"text_tokens": 5, "image_tokens": 15}}}),
        )
        .unwrap();
        assert_eq!(usage.tokens.input_tokens, Some(20));
        assert_eq!(usage.tokens.output_tokens, Some(8), "the total stays whole");
        assert_eq!(usage.metrics["image_output_tokens"], 8.into());
        assert_eq!(usage.metrics["image_input_tokens"], 15.into());
        assert_eq!(usage.metrics["image_outputs"], 2.into());
        assert_eq!(usage.dimensions[common::MODALITIES_IN_TOTALS], "true");
        assert_eq!(usage.dimensions["quality"], "high");

        let untokened = read(
            Operation::EditImage,
            Dialect::OpenAi,
            &json!({"data": [{"url": "https://x"}]}),
        )
        .unwrap();
        assert_eq!(untokened.metrics["image_outputs"], 1.into());
        assert_eq!(untokened.tokens.input_tokens, None);
        assert!(
            read(
                Operation::CreateImage,
                Dialect::OpenAi,
                &json!({"data": []})
            )
            .is_none()
        );
        assert!(whole(Operation::CreateImage, Dialect::OpenAi, b"{\"data\":[").is_none());
    }

    #[test]
    fn a_streamed_image_counts_its_completed_event_once() {
        let completed = json!({"type": "image_generation.completed", "generation_id": "g1",
            "b64_json": "QUJD", "size": "1024x1024", "quality": "high", "output_format": "png",
            "usage": {"input_tokens": 20, "output_tokens": 7,
                "input_tokens_details": {"text_tokens": 5, "image_tokens": 15},
                "output_tokens_details": {"image_tokens": 7}}});
        let partial = json!({"type": "image_generation.partial_image", "b64_json": "QQ==",
            "usage": {"input_tokens": 10, "output_tokens": 5}});
        let wire = format!(
            "event: image_generation.partial_image\ndata: {partial}\n\nevent: image_generation.completed\ndata: {completed}\n\ndata: {completed}\n\n"
        );
        for split in [1, 9, usize::MAX] {
            let usage = feed_sse(
                Operation::CreateImage,
                Dialect::OpenAi,
                &chunked(&wire, split),
                UsageStreamEnd::Complete,
            )
            .unwrap();
            assert_eq!(usage.tokens.input_tokens, Some(20));
            assert_eq!(usage.metrics["image_outputs"], 1.into());
            assert_eq!(usage.metrics["image_output_tokens"], 7.into());
            assert_eq!(usage.dimensions["output_format"], "png");
        }
        let only_partial = format!("data: {partial}\n\n");
        assert!(
            feed_sse(
                Operation::CreateImage,
                Dialect::OpenAi,
                &chunked(&only_partial, 4),
                UsageStreamEnd::Interrupted,
            )
            .is_none(),
            "a partial image is not an output"
        );
    }

    #[test]
    fn a_transcription_reports_tokens_or_seconds() {
        let tokens = read(
            Operation::CreateTranscription,
            Dialect::OpenAi,
            &json!({"text": "hi", "usage": {"type": "tokens", "input_tokens": 14, "output_tokens": 45,
                "total_tokens": 59, "input_token_details": {"text_tokens": 0, "audio_tokens": 14}}}),
        )
        .unwrap();
        assert_eq!(tokens.tokens.input_tokens, Some(14));
        assert_eq!(tokens.metrics["audio_input_tokens"], 14.into());
        let seconds = read(
            Operation::CreateTranslation,
            Dialect::OpenAi,
            &json!({"text": "hi", "usage": {"type": "duration", "seconds": 3.5}}),
        )
        .unwrap();
        assert_eq!(seconds.metrics["audio_seconds"], "3.5".parse().unwrap());
        assert_eq!(seconds.tokens.input_tokens, None);

        let stream = concat!(
            "data: {\"type\":\"transcript.text.delta\",\"delta\":\"transcript.text.done\"}\n\n",
            "data: {\"type\":\"transcript.text.done\",\"text\":\"hi\",\"usage\":{\"type\":\"tokens\",\"input_tokens\":3,\"output_tokens\":2}}\n\n",
        );
        let streamed = feed_sse(
            Operation::CreateTranscription,
            Dialect::OpenAi,
            &chunked(stream, 6),
            UsageStreamEnd::Complete,
        )
        .unwrap();
        assert_eq!(streamed.tokens.output_tokens, Some(2));
    }

    #[test]
    fn a_rerank_reports_input_and_search_units() {
        let tokens = read(
            Operation::Rerank,
            Dialect::OpenAi,
            &json!({"results": [], "usage": {"total_tokens": 31}}),
        )
        .unwrap();
        assert_eq!(tokens.tokens.input_tokens, Some(31));
        let units = read(
            Operation::Rerank,
            Dialect::OpenAi,
            &json!({"results": [], "usage": {"search_units": 1}}),
        )
        .unwrap();
        assert_eq!(units.metrics["search_units"], 1.into());
        assert!(read(Operation::Rerank, Dialect::OpenAi, &json!({"usage": {}})).is_none());
    }
}
