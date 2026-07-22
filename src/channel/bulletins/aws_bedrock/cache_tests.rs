use bytes::Bytes;
use http::{HeaderMap, StatusCode};
use serde_json::{Value, json};

use super::*;
use crate::protocol::{ContentGenerationKind, OperationKey};
use crate::transform::{TransformContext, dispatch, resolve};

const MAGIC: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH";

fn openai_chat_to_converse(body: &[u8], settings: &Value) -> Value {
    let source = OperationKey::content_generation(
        Operation::GenerateContent,
        ContentGenerationKind::OpenAiChatCompletions,
    );
    let target = OperationKey::content_generation(
        Operation::GenerateContent,
        ContentGenerationKind::ClaudeMessages,
    );
    let transformed = dispatch::request_bytes(
        resolve(source, target).unwrap(),
        &TransformContext::new(source, target),
        body,
    )
    .unwrap();
    let shaped = AwsBedrockChannel.shape_request(
        Bytes::from(transformed),
        &mut HeaderMap::new(),
        &ShapeCtx {
            op: target,
            stream: false,
            status: StatusCode::OK,
            settings,
        },
    );
    serde_json::from_slice(&shaped).unwrap()
}

#[test]
fn openai_breakpoint_becomes_converse_cache_point() {
    let value = openai_chat_to_converse(
        br#"{"model":"x","messages":[{"role":"developer","content":[{"type":"text","text":"stable","prompt_cache_breakpoint":{"mode":"explicit"}}]},{"role":"user","content":"hello"}]}"#,
        &json!({}),
    );
    assert_eq!(value["system"][0]["text"], "stable");
    assert_eq!(value["system"][1]["cachePoint"]["type"], "default");
}

#[test]
fn openai_magic_string_becomes_converse_cache_point() {
    let body = json!({
        "model": "x",
        "messages": [
            { "role": "developer", "content": format!("stable {MAGIC}") },
            { "role": "user", "content": "hello" }
        ]
    });
    let value = openai_chat_to_converse(
        body.to_string().as_bytes(),
        &json!({ "enable_openai_magic_cache": true }),
    );
    assert!(!value.to_string().contains(MAGIC));
    assert_eq!(value["system"][1]["cachePoint"]["type"], "default");
}
