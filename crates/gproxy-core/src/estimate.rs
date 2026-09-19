//! Local token estimation for exchanges whose upstream reported no usage, or
//! only part of it. Input tokens come from the text the request carried,
//! counted with the tokenizer selected for the upstream model (a custom
//! vocabulary from the model catalog, the Setting default, or the bundled
//! encoders); output tokens are a coarse half-of-bytes guess over the
//! response body. Everything estimated is marked `estimated=true` and
//! `Partial`; reported values are never overwritten.

use gproxy_channel::channel::{NormalizedUsage, UsageCompleteness};
use gproxy_tokenizer::{Tokenizer, Vocabulary};
use std::collections::HashMap;

/// Parsed custom vocabularies by file id plus which model uses which.
#[derive(Default)]
pub struct Estimator {
    vocabularies: HashMap<String, Vocabulary>,
    default_file: Option<String>,
    /// `(provider_id, upstream_name)` -> vocabulary file id.
    models: HashMap<(String, String), String>,
}

impl Estimator {
    pub(crate) fn new(
        vocabularies: HashMap<String, Vocabulary>,
        default_file: Option<String>,
        models: HashMap<(String, String), String>,
    ) -> Self {
        Self {
            vocabularies,
            default_file,
            models,
        }
    }

    /// Vocabularies already parsed, for reuse across reloads.
    pub(crate) fn vocabulary(&self, file_id: &str) -> Option<&Vocabulary> {
        self.vocabularies.get(file_id)
    }

    fn tokenizer(&self, provider_id: &str, model: Option<&str>) -> Tokenizer {
        let Some(model) = model else {
            return Tokenizer::character_estimate();
        };
        let file = self
            .models
            .get(&(provider_id.to_owned(), model.to_owned()))
            .or(self.default_file.as_ref())
            .and_then(|id| self.vocabularies.get(id));
        Tokenizer::for_model(model, file).unwrap_or_else(|_| Tokenizer::character_estimate())
    }

    /// Fill the token counts the upstream did not report. `response_bytes`
    /// is what was read of the body, whole or streamed.
    pub(crate) fn complete(
        &self,
        provider_id: &str,
        model: Option<&str>,
        request_body: Option<&[u8]>,
        response_bytes: u64,
        reported: Option<NormalizedUsage>,
    ) -> Option<NormalizedUsage> {
        let mut usage = reported.unwrap_or_default();
        let mut estimated = false;
        if usage.tokens.input_tokens.is_none()
            && let Some(body) = request_body
            && let Some(text) = request_text(body)
        {
            let tokenizer = self.tokenizer(provider_id, model);
            if let Ok(count) = tokenizer.count(&text) {
                usage.tokens.input_tokens = Some(count);
                estimated = true;
            }
        }
        if usage.tokens.output_tokens.is_none() && response_bytes > 0 {
            usage.tokens.output_tokens = Some(response_bytes.div_ceil(2));
            estimated = true;
        }
        if estimated {
            usage.completeness = UsageCompleteness::Partial;
            usage
                .dimensions
                .insert("estimated".to_owned(), "true".to_owned());
            Some(usage)
        } else if usage.tokens.input_tokens.is_some() || usage.tokens.output_tokens.is_some() {
            Some(usage)
        } else {
            None
        }
    }
}

/// The human-facing text of a request across the generate, count-tokens and
/// embeddings wire families: string values under the keys those bodies use
/// for prompts, messages, instructions and tool arguments, in document
/// order, joined by newlines. Ids, roles and control fields are skipped.
fn request_text(body: &[u8]) -> Option<String> {
    const TEXT_KEYS: &[&str] = &[
        "content",
        "text",
        "input",
        "instructions",
        "system",
        "arguments",
        "description",
        "output_text",
        "refusal",
        "prompt",
        "query",
        "documents",
    ];
    fn walk(value: &serde_json::Value, key: Option<&str>, out: &mut Vec<String>) {
        match value {
            serde_json::Value::String(text) => {
                if key.is_some_and(|k| TEXT_KEYS.contains(&k)) && !text.is_empty() {
                    out.push(text.clone());
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    walk(item, key, out);
                }
            }
            serde_json::Value::Object(map) => {
                for (k, v) in map {
                    walk(v, Some(k.as_str()), out);
                }
            }
            _ => {}
        }
    }
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let mut out = Vec::new();
    walk(&value, None, &mut out);
    (!out.is_empty()).then(|| out.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_text_collects_prompt_fields_only() {
        let body = br#"{"model":"m","messages":[{"role":"user","content":[{"type":"text","text":"hi there"},{"type":"image_url","image_url":{"url":"http://x"}}]},{"role":"assistant","content":"ok","tool_calls":[{"function":{"name":"f","arguments":"{\"a\":1}"}}]}],"tool_choice":"auto"}"#;
        assert_eq!(request_text(body).unwrap(), "hi there\nok\n{\"a\":1}");
        assert_eq!(request_text(br#"{"q":1}"#), None);
    }

    #[test]
    fn missing_counts_are_estimated_and_marked() {
        let estimator = Estimator::default();
        let usage = estimator
            .complete(
                "p",
                Some("gpt-4o"),
                Some(br#"{"input":"hello world"}"#),
                10,
                None,
            )
            .unwrap();
        assert_eq!(usage.tokens.input_tokens, Some(2));
        assert_eq!(usage.tokens.output_tokens, Some(5));
        assert_eq!(usage.completeness, UsageCompleteness::Partial);
        assert_eq!(usage.dimensions["estimated"], "true");
        let reported = NormalizedUsage {
            tokens: gproxy_channel::channel::TokenUsage {
                input_tokens: Some(7),
                output_tokens: Some(3),
                ..Default::default()
            },
            ..Default::default()
        };
        let kept = estimator
            .complete("p", Some("gpt-4o"), None, 100, Some(reported.clone()))
            .unwrap();
        assert_eq!(kept, reported, "reported values are never touched");
        assert!(estimator.complete("p", None, None, 0, None).is_none());
    }
}
