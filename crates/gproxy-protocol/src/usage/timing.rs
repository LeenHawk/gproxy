//! First generated content detection, using the usage reader's existing framing.
use serde_json::Value;

pub(super) fn has_output(name: Option<&str>, data: &str) -> bool {
    serde_json::from_str(data).is_ok_and(|value| value_has_output(name, &value))
}

pub(super) fn value_has_output(name: Option<&str>, value: &Value) -> bool {
    let nonempty = |value: &Value| value.as_str().is_some_and(|text| !text.is_empty());
    let kind = value["type"].as_str().or(name);
    match kind {
        Some("content_block_delta") => {
            let delta = &value["delta"];
            nonempty(&delta["text"])
                || nonempty(&delta["thinking"])
                || nonempty(&delta["partial_json"])
        }
        Some("content_block_start") => {
            let block = &value["content_block"];
            nonempty(&block["text"]) || nonempty(&block["thinking"])
        }
        Some(
            "response.output_text.delta"
            | "response.reasoning_text.delta"
            | "response.reasoning_summary_text.delta"
            | "response.function_call_arguments.delta"
            | "response.refusal.delta",
        ) => nonempty(&value["delta"]),
        Some(_) => false,
        None => {
            let chat = value["choices"].as_array().is_some_and(|choices| {
                choices.iter().any(|choice| {
                    let delta = &choice["delta"];
                    nonempty(&delta["content"])
                        || nonempty(&delta["reasoning_content"])
                        || nonempty(&delta["reasoning"])
                        || nonempty(&delta["refusal"])
                        || nonempty(&delta["function_call"]["arguments"])
                        || delta["tool_calls"].as_array().is_some_and(|calls| {
                            calls
                                .iter()
                                .any(|call| nonempty(&call["function"]["arguments"]))
                        })
                })
            });
            chat || value["candidates"].as_array().is_some_and(|candidates| {
                candidates.iter().any(|candidate| {
                    candidate["content"]["parts"]
                        .as_array()
                        .is_some_and(|parts| {
                            parts.iter().any(|part| {
                                nonempty(&part["text"])
                                    || part["functionCall"]
                                        .as_object()
                                        .is_some_and(|call| !call.is_empty())
                            })
                        })
                })
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ignores_metadata_and_recognizes_generated_content() {
        for event in [
            r#"{"type":"response.created"}"#,
            r#"{"choices":[{"delta":{"role":"assistant","content":""}}]}"#,
            r#"{"type":"message_start","message":{"usage":{"output_tokens":1}}}"#,
            "[DONE]",
        ] {
            assert!(!has_output(None, event));
        }
        for event in [
            r#"{"choices":[{"delta":{"content":"hello"}}]}"#,
            r#"{"type":"content_block_delta","delta":{"thinking":"hmm"}}"#,
            r#"{"type":"response.function_call_arguments.delta","delta":"{"}"#,
            r#"{"candidates":[{"content":{"parts":[{"text":"hello"}]}}]}"#,
        ] {
            assert!(has_output(None, event));
        }
    }
    #[test]
    fn fragmented_sse_waits_for_content_not_keepalive_or_role() {
        use crate::{
            Dialect, Operation,
            usage::{UsageReader, UsageTransport},
        };
        let mut reader = UsageReader::new(
            Operation::StreamGenerateContent,
            Dialect::OpenAiChat,
            UsageTransport::Http { framing: None },
        )
        .unwrap();
        reader
            .push(b": keepalive\n\ndata: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\n");
        assert!(!reader.output_started());
        reader.push(b"data: {\"choices\":[{\"delta\":{\"content\":\"hel");
        assert!(!reader.output_started());
        reader.push(b"lo\"}}]}\n\n");
        assert!(reader.output_started());
    }
}
