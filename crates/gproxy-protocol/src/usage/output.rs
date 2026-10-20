//! Visible generated characters for fallback metering. Never count wire framing,
//! media, signatures or response snapshots repeated after stream deltas.
use serde_json::Value;

use crate::{Dialect, Operation};

fn text(value: &Value) -> u64 {
    value.as_str().map_or(0, |text| text.chars().count() as u64)
}

fn items(value: &Value) -> impl Iterator<Item = &Value> {
    value.as_array().into_iter().flatten()
}

fn arguments(value: &Value) -> u64 {
    if value.is_null() || value.as_object().is_some_and(|object| object.is_empty()) {
        0
    } else if value.is_string() {
        text(value)
    } else {
        value.to_string().chars().count() as u64
    }
}

fn chat(message: &Value) -> u64 {
    // Compatible providers can repeat the same reasoning under both names.
    let reasoning = message["reasoning_content"]
        .as_str()
        .or_else(|| message["reasoning"].as_str())
        .map_or(0, |text| text.chars().count() as u64);
    text(&message["content"])
        + text(&message["refusal"])
        + reasoning
        + text(&message["function_call"]["arguments"])
        + items(&message["tool_calls"])
            .map(|call| text(&call["function"]["arguments"]))
            .sum::<u64>()
}

fn claude(block: &Value) -> u64 {
    match block["type"].as_str() {
        Some("text") => text(&block["text"]),
        Some("thinking") => text(&block["thinking"]),
        Some("tool_use" | "server_tool_use") => arguments(&block["input"]),
        _ => 0,
    }
}

fn gemini(value: &Value) -> u64 {
    items(&value["candidates"])
        .flat_map(|candidate| items(&candidate["content"]["parts"]))
        .map(|part| text(&part["text"]) + arguments(&part["functionCall"]["args"]))
        .sum()
}

pub(super) fn stream(dialect: Dialect, name: Option<&str>, value: &Value) -> u64 {
    let kind = value["type"].as_str().or(name);
    match dialect {
        Dialect::OpenAiChat => items(&value["choices"])
            .map(|choice| chat(&choice["delta"]))
            .sum(),
        Dialect::Gemini => gemini(value),
        Dialect::Claude => match kind {
            Some("content_block_start") => claude(&value["content_block"]),
            Some("content_block_delta") => {
                let delta = &value["delta"];
                text(&delta["text"]) + text(&delta["thinking"]) + text(&delta["partial_json"])
            }
            _ => 0,
        },
        Dialect::OpenAi | Dialect::OpenAiResponsesWebSocket => match kind {
            Some(
                "response.output_text.delta"
                | "response.reasoning_text.delta"
                | "response.reasoning_summary_text.delta"
                | "response.function_call_arguments.delta"
                | "response.custom_tool_call_input.delta"
                | "response.refusal.delta"
                | "transcript.text.delta"
                | "session.output_transcript.delta",
            ) => text(&value["delta"]),
            _ => 0,
        },
    }
}

/// Generated characters in a complete standard response. Unsupported bodies
/// yield zero; binary/media sizes are never a substitute for output tokens.
pub fn whole_output_characters(operation: Operation, dialect: Dialect, body: &[u8]) -> u64 {
    if !matches!(
        operation,
        Operation::GenerateContent
            | Operation::StreamGenerateContent
            | Operation::CompactContent
            | Operation::CreateDecision
            | Operation::CreateTranscription
            | Operation::CreateTranslation
    ) {
        return 0;
    }
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return 0;
    };
    if matches!(
        operation,
        Operation::CreateTranscription | Operation::CreateTranslation
    ) {
        return text(&value["text"]);
    }
    match dialect {
        Dialect::OpenAiChat => items(&value["choices"])
            .map(|choice| chat(&choice["message"]))
            .sum(),
        Dialect::Gemini => gemini(&value),
        Dialect::Claude => items(&value["content"]).map(claude).sum(),
        Dialect::OpenAi | Dialect::OpenAiResponsesWebSocket => items(&value["output"])
            .map(|item| match item["type"].as_str() {
                Some("message") => items(&item["content"])
                    .map(|part| match part["type"].as_str() {
                        Some("output_text") => text(&part["text"]),
                        Some("refusal") => text(&part["refusal"]),
                        _ => 0,
                    })
                    .sum(),
                Some("reasoning") => {
                    let content = item
                        .get("content")
                        .filter(|value| value.is_array())
                        .unwrap_or(&item["summary"]);
                    items(content).map(|part| text(&part["text"])).sum()
                }
                Some("function_call") => text(&item["arguments"]),
                Some("custom_tool_call") => text(&item["input"]),
                _ => 0,
            })
            .sum(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::{UsageReader, UsageTransport};

    #[test]
    fn fragmented_streams_count_content_without_metadata_or_terminal_replays() {
        for (dialect, event, replay) in [
            (
                Dialect::OpenAiChat,
                r#"{"choices":[{"delta":{"content":"你好","reasoning_content":"hmm","tool_calls":[{"function":{"arguments":"{}"}}]}}]}"#,
                r#"{"usage":{"completion_tokens":100}}"#,
            ),
            (
                Dialect::Claude,
                r#"{"type":"content_block_delta","delta":{"text":"你好hmm{}"}}"#,
                r#"{"type":"content_block_delta","delta":{"signature":"ignored"}}"#,
            ),
            (
                Dialect::OpenAi,
                r#"{"type":"response.output_text.delta","delta":"你好hmm{}"}"#,
                r#"{"type":"response.completed","response":{"output":[{"type":"message","content":[{"type":"output_text","text":"你好hmm{}"}]}]}}"#,
            ),
            (
                Dialect::Gemini,
                r#"{"candidates":[{"content":{"parts":[{"text":"你好hmm{}","thoughtSignature":"ignored"}]}}]}"#,
                r#"{"candidates":[{"content":{"parts":[{"inlineData":{"data":"ignored"}}]}}]}"#,
            ),
        ] {
            for framing in [
                crate::connection::StreamFraming::Sse,
                crate::connection::StreamFraming::JsonArray,
            ] {
                if framing == crate::connection::StreamFraming::JsonArray
                    && dialect != Dialect::Gemini
                {
                    continue;
                }
                let mut reader = UsageReader::new(
                    Operation::StreamGenerateContent,
                    dialect,
                    UsageTransport::Http {
                        framing: Some(framing),
                    },
                )
                .unwrap();
                reader.count_output_characters();
                let wire = if framing == crate::connection::StreamFraming::Sse {
                    format!(
                        ": {}\n\ndata: {event}\n\ndata: {replay}\n\ndata: [DONE]\n\n",
                        "keepalive".repeat(1024)
                    )
                } else {
                    format!("[{event},{replay}]")
                };
                for chunk in wire.as_bytes().chunks(3) {
                    reader.push(chunk);
                }
                assert_eq!(reader.output_characters(), 7, "{dialect:?} {framing:?}");
            }
        }
    }

    #[test]
    fn whole_responses_count_text_thoughts_and_arguments_only() {
        for (dialect, body) in [
            (
                Dialect::OpenAiChat,
                r#"{"choices":[{"message":{"content":"你好","reasoning":"hmm","tool_calls":[{"function":{"arguments":"{}"}}],"audio":{"data":"ignored"}}}]}"#,
            ),
            (
                Dialect::Claude,
                r#"{"content":[{"type":"text","text":"你好"},{"type":"thinking","thinking":"hmm","signature":"ignored"},{"type":"tool_use","input":"{}"}]}"#,
            ),
            (
                Dialect::OpenAi,
                r#"{"output":[{"type":"message","content":[{"type":"output_text","text":"你好"}]},{"type":"reasoning","summary":[{"text":"hmm"}],"encrypted_content":"ignored"},{"type":"function_call","arguments":"{}"}]}"#,
            ),
            (
                Dialect::Gemini,
                r#"{"candidates":[{"content":{"parts":[{"text":"你好"},{"text":"hmm","thought":true},{"functionCall":{"args":"{}"}},{"inlineData":{"data":"ignored"}}]}}]}"#,
            ),
        ] {
            assert_eq!(
                whole_output_characters(Operation::GenerateContent, dialect, body.as_bytes()),
                7
            );
        }
        assert_eq!(
            whole_output_characters(Operation::CreateSpeech, Dialect::OpenAi, b"audio bytes"),
            0
        );
        assert_eq!(
            whole_output_characters(
                Operation::GenerateContent,
                Dialect::OpenAi,
                br#"{"error":{"message":"bad request"}}"#
            ),
            0
        );
    }
}
