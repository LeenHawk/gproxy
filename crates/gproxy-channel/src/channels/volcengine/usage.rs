//! image-generation-api, get-video-generation-task-api and HTTP TTS usage.
//! Keep image-only output tokens and billed characters even when the vendor
//! omits the input/output pair a standard OpenAI usage reader expects.
use super::{Volcengine, audio::CHARACTERS_HEADER};
use crate::channel::{NormalizedUsage, UsageCompleteness, UsageExtras, UsageSource, usage_object};
use gproxy_protocol::Operation;
use serde_json::Value;

impl UsageExtras for Volcengine {
    fn read(&self, source: UsageSource<'_>, usage: &mut NormalizedUsage) {
        if source.operation.operation == Operation::CreateSpeech
            && let Some(words) = source
                .headers
                .get(CHARACTERS_HEADER)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
        {
            usage
                .metrics
                .insert("audio_characters".into(), words.into());
            usage.completeness = UsageCompleteness::Complete;
        }
        let Some(raw) = usage_object(source.root) else {
            return;
        };
        if source.operation.operation == Operation::CreateImage {
            if let Some(tokens) = raw.get("output_tokens").and_then(Value::as_u64) {
                usage.tokens.output_tokens = Some(tokens);
                usage
                    .metrics
                    .insert("image_output_tokens".into(), tokens.into());
                usage
                    .dimensions
                    .insert("token_modalities_in_totals".into(), "true".into());
            }
            if let Some(images) = raw.get("generated_images").and_then(Value::as_u64) {
                usage.metrics.insert("image_outputs".into(), images.into());
            }
        }
        if source.operation.operation == Operation::RetrieveVideo
            && let Some(tokens) = raw.get("completion_tokens").and_then(Value::as_u64)
        {
            usage.tokens.input_tokens = Some(0); // Explicit in the video API contract.
            usage.tokens.output_tokens = Some(tokens);
        }
        if let Some(searches) = raw
            .pointer("/tool_usage/web_search")
            .and_then(Value::as_u64)
        {
            usage.metrics.insert("web_searches".into(), searches.into());
        }
    }
}
