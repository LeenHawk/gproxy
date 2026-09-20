#![recursion_limit = "256"]

use gproxy_protocol::openai::chat::content::{ChatMessage, UserContentPart};
use gproxy_protocol::openai::chat::request::GenerateContentRequestBody;
use gproxy_protocol::openai::chat::response::ResponseRole;
use gproxy_protocol::openai::chat::response::{
    GenerateContentResponseBody, ModerationResultOrError,
};
use gproxy_protocol::openai::chat::tools::{ChatTool, ChatToolType, ToolChoice};

fn request_fixture() -> serde_json::Value {
    serde_json::json!({
        "messages":[
            {"role":"developer","content":[{"type":"text","text":"rules"}]},
            {"role":"system","content":"system"},
            {"role":"user","content":[{"type":"text","text":"look"},{"type":"image_url","image_url":{"url":"https://example.test/a.png","detail":"high"}},{"type":"input_audio","input_audio":{"data":"AA==","format":"wav"}},{"type":"file","file":{"file_id":"file_1","filename":"a.txt"}}]},
            {"role":"assistant","content":[{"type":"refusal","refusal":"no"}],"refusal":null,"tool_calls":[{"id":"call_1","type":"function","function":{"name":"lookup","arguments":"{}"}}]},
            {"role":"tool","content":"result","tool_call_id":"call_1"},
            {"role":"function","content":null,"name":"lookup"}
        ],
        "model":"gpt-5",
        "audio":{"voice":"alloy","format":"wav"},
        "frequency_penalty":0.2,"function_call":"auto","functions":[{"name":"lookup","parameters":{"type":"object"}}],
        "logit_bias":{"42":-1.0},"logprobs":true,"max_completion_tokens":100,"max_tokens":120,
        "metadata":{"tenant":"test"},"modalities":["text","audio"],"moderation":{"model":"omni-moderation-latest","policy":{"input":{"mode":"score"}}},
        "n":1,"parallel_tool_calls":true,"prediction":{"type":"content","content":"expected"},"presence_penalty":0.1,
        "prompt_cache_key":"cache","prompt_cache_options":{"mode":"explicit","ttl":"30m"},"prompt_cache_retention":"24h","reasoning_effort":"high",
        "response_format":{"type":"json_schema","json_schema":{"name":"answer","schema":{"type":"object"},"strict":true}},
        "safety_identifier":"user-1","seed":7,"service_tier":"priority","stop":["END"],"store":true,"stream":false,
        "stream_options":{"include_obfuscation":true,"include_usage":true},"temperature":0.3,"tool_choice":"required",
        "tools":[{"type":"function","function":{"name":"lookup","description":"find","parameters":{"type":"object"},"strict":true}}],
        "top_logprobs":5,"top_p":0.9,"user":"legacy","verbosity":"medium","web_search_options":{"search_context_size":"high","user_location":{"type":"approximate","approximate":{"city":"Paris"}}},"future":true
    })
}

#[test]
fn chat_request_all_roles_modalities_tools_and_options_round_trip() {
    let value = request_fixture();
    for (index, message) in value["messages"].as_array().unwrap().iter().enumerate() {
        if index == 2 {
            for (part_index, part) in message["content"].as_array().unwrap().iter().enumerate() {
                serde_json::from_value::<UserContentPart>(part.clone())
                    .unwrap_or_else(|error| panic!("part {part_index}: {error}"));
            }
        }
        serde_json::from_value::<ChatMessage>(message.clone())
            .unwrap_or_else(|error| panic!("message {index}: {error}"));
    }
    let parsed: GenerateContentRequestBody = serde_json::from_value(value.clone()).unwrap();
    assert!(matches!(parsed.messages[0], ChatMessage::Developer(_)));
    assert!(matches!(parsed.messages[2], ChatMessage::User(_)));
    assert!(matches!(parsed.tool_choice, Some(ToolChoice::Mode(_))));
    assert!(matches!(
        parsed.tools.as_ref().unwrap()[0],
        ChatTool::Function(_)
    ));
    assert!(parsed.rest.contains_key("future"));
    assert_eq!(serde_json::to_value(parsed).unwrap(), value);
}

fn response_fixture() -> serde_json::Value {
    serde_json::json!({
        "id":"chatcmpl_1","choices":[{"finish_reason":"stop","index":0,"logprobs":{"content":[{"token":"hello","bytes":[104],"logprob":-0.1,"top_logprobs":[{"token":"hello","bytes":[104],"logprob":-0.1}]}],"refusal":null},"message":{"content":"hello","refusal":null,"role":"assistant","annotations":[{"type":"url_citation","url_citation":{"end_index":5,"start_index":0,"title":"Example","url":"https://example.test"}}],"audio":{"id":"audio_1","data":"AA==","expires_at":123,"transcript":"hello"},"function_call":{"name":"lookup","arguments":"{}"},"tool_calls":[{"id":"call_1","type":"function","function":{"name":"lookup","arguments":"{}"}}]}}],
        "created":123,"model":"gpt-5","object":"chat.completion","service_tier":"priority","system_fingerprint":"fp","usage":{"prompt_tokens":4,"completion_tokens":2,"total_tokens":6,"prompt_tokens_details":{"cached_tokens":1,"audio_tokens":0},"completion_tokens_details":{"reasoning_tokens":1,"audio_tokens":0}},"moderation":{"input":{"model":"omni-moderation-latest","type":"moderation_results","results":[{"categories":{"violence":false},"category_applied_input_types":{"violence":["text"]},"category_scores":{"violence":0.01},"flagged":false,"model":"omni-moderation-latest","type":"moderation_result"}]},"output":{"code":"none","message":"not requested","type":"error"}},"future_response":true
    })
}

#[test]
fn chat_response_choices_logprobs_usage_and_nullable_message_fields_are_typed() {
    let value = response_fixture();
    let parsed: GenerateContentResponseBody = serde_json::from_value(value.clone()).unwrap();
    assert!(matches!(
        parsed.choices[0].message.role,
        ResponseRole::Assistant
    ));
    assert_eq!(
        parsed.choices[0]
            .logprobs
            .as_ref()
            .unwrap()
            .content
            .as_ref()
            .unwrap()[0]
            .token,
        "hello"
    );
    assert_eq!(parsed.usage.as_ref().unwrap().total_tokens, 6);
    assert!(
        matches!(parsed.moderation.as_ref().unwrap().as_ref().unwrap().input, ModerationResultOrError::Result(ref result) if !result.results[0].flagged)
    );
    assert!(parsed.rest.contains_key("future_response"));
    assert_eq!(serde_json::to_value(parsed).unwrap(), value);
}

#[test]
fn chat_closed_enums_and_direct_builder_are_supported() {
    assert!(serde_json::from_value::<ChatToolType>(serde_json::json!("future")).is_err());
    let part = UserContentPart::Text(
        serde_json::from_value(serde_json::json!({"text":"x","type":"text"})).unwrap(),
    );
    assert_eq!(serde_json::to_value(part).unwrap()["type"], "text");
}

#[test]
fn every_known_fixture_field_is_typed() {
    let mut request = request_fixture();
    request.as_object_mut().unwrap().remove("future");
    let request: GenerateContentRequestBody = serde_json::from_value(request).unwrap();
    assert!(
        format!("{request:?}")
            .split("rest: ")
            .skip(1)
            .all(|rest| rest.starts_with("{}")),
        "{request:?}"
    );
    let mut response = response_fixture();
    response.as_object_mut().unwrap().remove("future_response");
    response["usage"]["completion_tokens_details"]["accepted_prediction_tokens"] =
        serde_json::json!(2);
    response["usage"]["completion_tokens_details"]["rejected_prediction_tokens"] =
        serde_json::json!(3);
    response["usage"]["prompt_tokens_details"]["cache_write_tokens"] = serde_json::json!(4);
    let parsed: GenerateContentResponseBody = serde_json::from_value(response.clone()).unwrap();
    assert!(
        format!("{parsed:?}")
            .split("rest: ")
            .skip(1)
            .all(|rest| rest.starts_with("{}")),
        "{parsed:?}"
    );
    assert_eq!(serde_json::to_value(parsed).unwrap(), response);
}

#[test]
fn optional_nullable_fields_preserve_missing_null_and_values() {
    let request = request_fixture();
    for path in [
        "/audio",
        "/frequency_penalty",
        "/logit_bias",
        "/logprobs",
        "/max_completion_tokens",
        "/max_tokens",
        "/metadata",
        "/modalities",
        "/moderation",
        "/n",
        "/prediction",
        "/presence_penalty",
        "/prompt_cache_key",
        "/prompt_cache_retention",
        "/reasoning_effort",
        "/safety_identifier",
        "/seed",
        "/service_tier",
        "/stop",
        "/store",
        "/stream",
        "/stream_options",
        "/temperature",
        "/top_logprobs",
        "/top_p",
        "/verbosity",
        "/moderation/policy",
        "/moderation/policy/input",
        "/response_format/json_schema/strict",
        "/web_search_options/user_location",
        "/tools/0/function/strict",
    ] {
        let (parent, key) = path.rsplit_once('/').unwrap();
        let mut missing = request.clone();
        missing
            .pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(key);
        assert_eq!(
            serde_json::to_value(
                serde_json::from_value::<GenerateContentRequestBody>(missing.clone()).unwrap()
            )
            .unwrap(),
            missing,
            "missing {path}"
        );
        let mut null = request.clone();
        *null.pointer_mut(path).unwrap() = serde_json::Value::Null;
        assert_eq!(
            serde_json::to_value(
                serde_json::from_value::<GenerateContentRequestBody>(null.clone()).unwrap()
            )
            .unwrap(),
            null,
            "null {path}"
        );
    }
    for path in ["/service_tier", "/moderation", "/choices/0/message/audio"] {
        let value = response_fixture();
        let (parent, key) = path.rsplit_once('/').unwrap();
        let mut missing = value.clone();
        missing
            .pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(key);
        assert_eq!(
            serde_json::to_value(
                serde_json::from_value::<GenerateContentResponseBody>(missing.clone()).unwrap()
            )
            .unwrap(),
            missing
        );
        let mut null = value;
        *null.pointer_mut(path).unwrap() = serde_json::Value::Null;
        assert_eq!(
            serde_json::to_value(
                serde_json::from_value::<GenerateContentResponseBody>(null.clone()).unwrap()
            )
            .unwrap(),
            null
        );
    }
}

#[test]
fn required_nullable_fields_reject_missing_and_keep_null() {
    for path in [
        "/choices/0/logprobs/content",
        "/choices/0/logprobs/refusal",
        "/choices/0/logprobs/content/0/bytes",
        "/choices/0/logprobs/content/0/top_logprobs/0/bytes",
    ] {
        let value = response_fixture();
        let (parent, key) = path.rsplit_once('/').unwrap();
        let mut missing = value.clone();
        missing
            .pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(key);
        assert!(
            serde_json::from_value::<GenerateContentResponseBody>(missing).is_err(),
            "missing {path}"
        );
        let mut null = value;
        *null.pointer_mut(path).unwrap() = serde_json::Value::Null;
        assert_eq!(
            serde_json::to_value(
                serde_json::from_value::<GenerateContentResponseBody>(null.clone()).unwrap()
            )
            .unwrap(),
            null
        );
    }
    assert!(
        serde_json::from_value::<ChatMessage>(serde_json::json!({"role":"function","name":"f"}))
            .is_err()
    );
}

#[test]
fn unions_respect_labels_required_payloads_and_native_chat_shapes() {
    use gproxy_protocol::openai::chat::*;
    for value in [
        serde_json::json!({"role":"assistant","tool_calls":[{"id":"c","type":"custom","custom":{"name":"c","input":"raw"},"function":{"name":"extension","arguments":"{}"}}]}),
        serde_json::json!({"role":"assistant","content":"text","audio":null,"function_call":null,"refusal":null}),
        serde_json::json!({"role":"tool","content":[{"type":"text","text":"ok"}],"tool_call_id":"t"}),
    ] {
        let parsed: ChatMessage = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), value);
    }
    for value in [
        serde_json::json!({"type":"custom","custom":{"name":"c","format":{"type":"grammar","grammar":{"syntax":"regex","definition":".*"}}},"function":{"name":"extension"}}),
        serde_json::json!({"type":"function","function":{"name":"f","strict":null}}),
    ] {
        let parsed: ChatTool = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), value);
    }
    for value in [
        serde_json::json!({"type":"function"}),
        serde_json::json!({"type":"custom","function":{"name":"f"}}),
    ] {
        assert!(serde_json::from_value::<ChatTool>(value).is_err());
    }
    for role in ["developer", "system", "tool"] {
        assert!(serde_json::from_value::<ChatMessage>(serde_json::json!({"role":role,"tool_call_id":"t","content":[{"type":"refusal","text":"extension","refusal":"no"}]})).is_err());
    }
    assert!(
        serde_json::from_value::<Prediction>(
            serde_json::json!({"type":"content","content":[{"type":"refusal","refusal":"no"}]})
        )
        .is_err()
    );
    for value in [
        serde_json::json!({"type":"allowed_tools","allowed_tools":{"mode":"auto","tools":[{"type":"function","function":{"name":"f"}}]}}),
        serde_json::json!({"type":"custom","custom":{"name":"c"}}),
        serde_json::json!({"type":"function","function":{"name":"f"}}),
    ] {
        let parsed: ToolChoice = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), value);
    }
    let mut response = response_fixture();
    response["choices"][0]["message"]["tool_calls"] =
        serde_json::json!([{"id":"c","type":"custom","custom":{"name":"c","input":"raw"}}]);
    assert_eq!(
        serde_json::to_value(
            serde_json::from_value::<GenerateContentResponseBody>(response.clone()).unwrap()
        )
        .unwrap(),
        response
    );
}

#[test]
fn typed_builders_keep_transport_and_required_tags() {
    use gproxy_protocol::openai::chat::*;
    use gproxy_protocol::{WireRequest, WireResponse};
    let text = TextPart::builder(TextPartType::Text, "hello".into()).build();
    let message = ChatMessage::Developer(
        DeveloperMessage::builder(DeveloperRole::Developer, TextContent::Parts(vec![text])).build(),
    );
    let request: GenerateContentRequest = WireRequest {
        method: http::Method::POST,
        path: "/v1/chat/completions".into(),
        query: Some("trace=1".into()),
        headers: http::HeaderMap::new(),
        body: GenerateContentRequestBody::builder(vec![message], "gpt-5".into()).build(),
    };
    assert_eq!(request.method, http::Method::POST);
    assert_eq!(request.path, "/v1/chat/completions");
    assert_eq!(request.query.as_deref(), Some("trace=1"));
    assert!(request.headers.is_empty());
    assert_eq!(
        serde_json::to_value(request.body).unwrap(),
        serde_json::json!({"model":"gpt-5","messages":[{"role":"developer","content":[{"type":"text","text":"hello"}]}]})
    );
    let message = ResponseMessage::builder(None, None, ResponseRole::Assistant).build();
    let choice = Choice::builder(FinishReason::Stop, 0, None, message).build();
    let response: GenerateContentResponse = WireResponse {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: GenerateContentResponseBody::builder(
            "c".into(),
            vec![choice],
            1,
            "gpt-5".into(),
            CompletionObject::ChatCompletion,
        )
        .build(),
    };
    assert_eq!(response.status, http::StatusCode::OK);
    assert!(response.headers.is_empty());
    assert_eq!(
        serde_json::to_value(response.body).unwrap(),
        serde_json::json!({"id":"c","choices":[{"finish_reason":"stop","index":0,"logprobs":null,"message":{"role":"assistant","content":null,"refusal":null}}],"created":1,"model":"gpt-5","object":"chat.completion"})
    );
}

#[test]
fn official_closed_enum_values_round_trip() {
    use gproxy_protocol::openai::chat::*;
    use serde::{Serialize, de::DeserializeOwned};
    fn values<T: Serialize + DeserializeOwned>(values: &[&str]) {
        for value in values {
            let parsed: T = serde_json::from_value(serde_json::json!(value)).unwrap();
            assert_eq!(
                serde_json::to_value(parsed).unwrap(),
                serde_json::json!(value)
            );
        }
        assert!(serde_json::from_value::<T>(serde_json::json!("UNKNOWN")).is_err());
    }
    values::<ImageDetail>(&["auto", "low", "high"]);
    values::<AudioFormat>(&["wav", "mp3"]);
    values::<ChatAudioFormat>(&["wav", "aac", "mp3", "flac", "opus", "pcm16"]);
    values::<Modality>(&["text", "audio"]);
    values::<ModerationModeType>(&["score", "block"]);
    values::<PromptCacheOptionMode>(&["implicit", "explicit"]);
    values::<PromptCacheTtl>(&["30m"]);
    values::<PromptCacheRetention>(&["in_memory", "24h"]);
    values::<ReasoningEffort>(&["none", "minimal", "low", "medium", "high", "xhigh", "max"]);
    values::<ServiceTier>(&["auto", "default", "flex", "scale", "priority", "fast"]);
    values::<Verbosity>(&["low", "medium", "high"]);
    values::<SearchContextSize>(&["low", "medium", "high"]);
    values::<GrammarSyntax>(&["lark", "regex"]);
    values::<FinishReason>(&[
        "stop",
        "length",
        "tool_calls",
        "content_filter",
        "function_call",
    ]);
    values::<ResponseServiceTier>(&["auto", "default", "flex", "scale", "priority", "fast"]);
    values::<ModerationInputType>(&["text", "image"]);
}

#[test]
fn remaining_native_variants_and_nullable_assistant_fields_round_trip() {
    use gproxy_protocol::openai::chat::*;
    for value in [
        serde_json::json!({"messages":[],"model":"gpt-5","audio":{"voice":{"id":"voice_1"},"format":"aac"},"function_call":{"name":"f"},"response_format":{"type":"text"},"stop":"END","moderation":{"model":"m","policy":{"output":null}}}),
        serde_json::json!({"messages":[],"model":"gpt-5","audio":{"voice":"future-voice","format":"pcm16"},"response_format":{"type":"json_object"},"prediction":{"type":"content","content":[{"type":"text","text":"expected"}]},"tools":[{"type":"custom","custom":{"name":"c","format":{"type":"text"}}}]}),
        serde_json::json!({"messages":[],"model":"gpt-5","tools":[{"type":"custom","custom":{"name":"c","format":{"type":"grammar","grammar":{"syntax":"lark","definition":"start: WORD"}}}}]}),
    ] {
        let parsed: GenerateContentRequestBody = serde_json::from_value(value.clone()).unwrap();
        assert!(
            format!("{parsed:?}")
                .split("rest: ")
                .skip(1)
                .all(|rest| rest.starts_with("{}"))
        );
        assert_eq!(serde_json::to_value(parsed).unwrap(), value);
    }
    let assistant = serde_json::json!({"role":"assistant","content":"text","audio":{"id":"a"},"refusal":"refusal","function_call":{"arguments":"{}","name":"f"}});
    for key in ["content", "audio", "refusal", "function_call"] {
        let mut missing = assistant.clone();
        missing.as_object_mut().unwrap().remove(key);
        assert_eq!(
            serde_json::to_value(serde_json::from_value::<ChatMessage>(missing.clone()).unwrap())
                .unwrap(),
            missing
        );
        let mut null = assistant.clone();
        null[key] = serde_json::Value::Null;
        assert_eq!(
            serde_json::to_value(serde_json::from_value::<ChatMessage>(null.clone()).unwrap())
                .unwrap(),
            null
        );
    }
    for value in [
        serde_json::json!({"name":"f","parameters":[]}),
        serde_json::json!({"name":"f","parameters":true}),
    ] {
        assert!(serde_json::from_value::<FunctionDefinition>(value.clone()).is_err());
        assert!(serde_json::from_value::<LegacyFunction>(value).is_err());
    }
    assert!(
        serde_json::from_value::<JsonSchemaFormat>(serde_json::json!({"name":"s","schema":[]}))
            .is_err()
    );
    assert!(
        serde_json::from_value::<AllowedTools>(serde_json::json!({"mode":"auto","tools":[1]}))
            .is_err()
    );
    for key in ["input", "output"] {
        let mut response = response_fixture();
        response["moderation"].as_object_mut().unwrap().remove(key);
        assert!(serde_json::from_value::<GenerateContentResponseBody>(response).is_err());
    }
}
