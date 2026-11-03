use super::*;
use crate::support;
pub(crate) fn document(source: DocumentSource) -> Document {
    Document {
        source,
        media_type: Some("application/pdf".into()),
        title: Some("Report".into()),
        context: Some("Notes".into()),
        citations: Some(true),
        detail: Detail::High,
    }
}

pub(crate) fn gemini(parts: Value) -> GenerateResult {
    dialects::gemini::parse(serde_json::from_value(json!({"candidates":[{"content":{"role":"model","parts":parts},"finishReason":"STOP"}]})).unwrap()).unwrap()
}

pub(crate) fn replay_gemini(parts: Value) {
    let result = gemini(parts.clone());
    let request = support::request(Dialect::Gemini, support::persisted(result));
    assert_eq!(
        dialects::render(&request, false).unwrap().request["contents"][0]["parts"],
        parts
    );
}

pub(crate) fn chat_response() -> Value {
    json!({
        "id": "chat_1",
        "object": "chat.completion",
        "created": 1,
        "model": "audio-model",
        "choices": [
            {
                "index": 0,
                "finish_reason": "tool_calls",
                "message": {
                    "role": "assistant",
                    "content": "hello",
                    "refusal": null,
                    "audio": {
                        "id": "audio_1",
                        "expires_at": 2000000000,
                        "data": "AQID",
                        "transcript": "spoken hello",
                        "extra": 1
                    },
                    "tool_calls": [
                        {
                            "id": "call_1",
                            "type": "function",
                            "function": {
                                "name": "weather",
                                "arguments": "{ \"city\": \"Shanghai\" }",
                                "extra": 2
                            },
                            "extra": 3
                        }
                    ],
                    "extra": 4
                }
            }
        ],
        "usage": {
            "prompt_tokens": 8,
            "completion_tokens": 9,
            "total_tokens": 17
        }
    })
}
