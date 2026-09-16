use gproxy_protocol::{
    Dialect,
    transform::{TransformErrorKind, generate::chat_responses::*, identity::*},
    wire::openai::{
        chat::response as c,
        responses::{input as i, response as r},
    },
};
use serde_json::{Value, json};
fn flow(byte: u8) -> IdentityFlow {
    IdentityFlow::new(IdNamespace::with_bytes([byte; 16]))
}
fn policy(dialect: Dialect) -> TargetIdPolicy {
    TargetIdPolicy::new(dialect)
}
fn context() -> ResponsesResponseContext {
    ResponsesResponseContext {
        request:serde_json::from_value(json!({"model":"requested","instructions":"rules","metadata":{"trace":"a"},"parallel_tool_calls":false,"temperature":0.2,"top_p":0.8,"tool_choice":"required","tools":[{"type":"function","name":"lookup","description":null,"parameters":{"type":"object","future":7},"strict":false,"future":3}],"future":1})).unwrap(),
        effective_parallel_tool_calls:false,effective_tool_choice:i::ToolChoice::Mode(i::ToolChoiceMode::Required),
        usage:ChatUsageSupplement::default(),effective_prompt_cache_options:None,
    }
}
fn chat() -> Value {
    json!({"id":"chatcmpl_native","created":123,"model":"actual-model","object":"chat.completion","choices":[{"index":0,"finish_reason":"stop","logprobs":null,"message":{"role":"assistant","content":"hello","refusal":"also refused","future":2}}],"service_tier":"priority","usage":{"prompt_tokens":12,"completion_tokens":5,"total_tokens":17,"prompt_tokens_details":{"cached_tokens":3,"cache_write_tokens":2,"future":4},"completion_tokens_details":{"reasoning_tokens":1}},"future":1})
}
fn convert(value: Value, namespace: u8) -> r::GenerateContentResponseBody {
    chat_to_responses_response(
        serde_json::from_value(value).unwrap(),
        context(),
        &mut flow(namespace),
        &policy(Dialect::OpenAi),
    )
    .unwrap()
    .value
}
#[test]
fn response_keeps_request_facts_actual_usage_and_both_text_and_refusal() {
    let output = convert(chat(), 1);
    let value = serde_json::to_value(&output).unwrap();
    assert_eq!(value["model"], "actual-model");
    assert_eq!(value["instructions"], "rules");
    assert_eq!(value["parallel_tool_calls"], false);
    assert_eq!(value["tool_choice"], "required");
    assert_eq!(value["temperature"], 0.2);
    assert_eq!(value["tools"][0]["name"], "lookup");
    assert_eq!(value["tools"][0]["parameters"]["future"], 7);
    assert!(value["tools"][0].get("future").is_none());
    assert_eq!(value["output"][0]["content"][0]["text"], "hello");
    assert_eq!(value["output"][0]["content"][1]["refusal"], "also refused");
    assert_eq!(value["usage"]["input_tokens_details"]["cached_tokens"], 3);
    assert_eq!(
        value["usage"]["input_tokens_details"]["cache_write_tokens"],
        2
    );
    assert_eq!(
        value["usage"]["output_tokens_details"]["reasoning_tokens"],
        1
    );
    assert!(value.get("future").is_none());
    assert!(value["output"][0]["content"][0].get("future").is_none());
    let back = responses_to_chat_response(output, &mut flow(2), &policy(Dialect::OpenAiChat))
        .unwrap()
        .value;
    assert_eq!(back.choices[0].message.content.as_deref(), Some("hello"));
    assert_eq!(
        back.choices[0].message.refusal.as_deref(),
        Some("also refused")
    );
    assert_eq!(
        back.usage
            .unwrap()
            .prompt_tokens_details
            .unwrap()
            .cached_tokens,
        Some(3)
    );
    assert_eq!(
        back.service_tier,
        Some(Some(c::ResponseServiceTier::Priority))
    );
}
#[test]
fn generated_ids_are_scoped_stable_and_distinct_from_call_ids() {
    let mut input = chat();
    input["choices"][0]["finish_reason"] = json!("tool_calls");
    input["choices"][0]["message"]["tool_calls"] = json!([
        {"type":"function","id":"call_one","function":{"name":"lookup","arguments":"{invalid","future":1},"future":2},
        {"type":"function","id":"call_two","function":{"name":"lookup","arguments":"[]"}},
        {"type":"custom","id":"call_custom","custom":{"name":"shell","input":"echo hi","future":3}}
    ]);
    let first = serde_json::to_value(convert(input.clone(), 3)).unwrap();
    let retry = serde_json::to_value(convert(input.clone(), 3)).unwrap();
    assert_eq!(first, retry);
    let other = serde_json::to_value(convert(input, 4)).unwrap();
    assert_ne!(first["output"][0]["id"], other["output"][0]["id"]);
    assert_eq!(first["output"][1]["call_id"], "call_one");
    assert_ne!(first["output"][1]["id"], first["output"][1]["call_id"]);
    assert_ne!(first["output"][1]["id"], first["output"][2]["id"]);
    assert_eq!(first["output"][1]["arguments"], "{invalid");
    assert!(
        first["output"][1]["id"]
            .as_str()
            .unwrap()
            .starts_with("fc_")
    );
    assert!(
        first["output"][3]["id"]
            .as_str()
            .unwrap()
            .starts_with("ctc_")
    );
    let back = responses_to_chat_response(
        serde_json::from_value(first).unwrap(),
        &mut flow(5),
        &policy(Dialect::OpenAiChat),
    )
    .unwrap()
    .value;
    assert_eq!(back.choices[0].finish_reason, c::FinishReason::ToolCalls);
    assert_eq!(
        back.choices[0].message.tool_calls.as_ref().unwrap().len(),
        3
    );
}
#[test]
fn length_and_filter_are_not_successful_stop() {
    for (chat_reason, response_reason) in [
        ("length", "max_output_tokens"),
        ("content_filter", "content_filter"),
    ] {
        let mut input = chat();
        input["choices"][0]["finish_reason"] = json!(chat_reason);
        let response = convert(input, 6);
        let value = serde_json::to_value(&response).unwrap();
        assert_eq!(value["status"], "incomplete");
        assert_eq!(value["incomplete_details"]["reason"], response_reason);
        assert_eq!(value["output"][0]["status"], "incomplete");
        let back = responses_to_chat_response(response, &mut flow(7), &policy(Dialect::OpenAiChat))
            .unwrap()
            .value;
        assert_eq!(
            serde_json::to_value(back.choices[0].finish_reason).unwrap(),
            chat_reason
        );
    }
    let mut response = convert(chat(), 8);
    response.status = Some(r::ResponseStatus::Failed);
    assert_eq!(
        responses_to_chat_response(response, &mut flow(9), &policy(Dialect::OpenAiChat))
            .unwrap_err()
            .kind(),
        TransformErrorKind::InvalidResult
    );
}
#[test]
fn missing_usage_detail_requires_real_facts_and_errors_do_not_mutate_ids() {
    let mut input = chat();
    input["usage"]
        .as_object_mut()
        .unwrap()
        .remove("prompt_tokens_details");
    let mut ids = flow(10);
    let expected = ids.clone();
    let output = chat_to_responses_response(
        serde_json::from_value(input.clone()).unwrap(),
        context(),
        &mut ids,
        &policy(Dialect::OpenAi),
    )
    .unwrap();
    assert!(output.value.usage.is_none());
    assert_eq!(ids.namespace(), expected.namespace());
    let mut facts = context();
    facts.usage = ChatUsageSupplement {
        cached_tokens: Some(3),
        cache_write_tokens: Some(2),
        reasoning_tokens: None,
    };
    let output = chat_to_responses_response(
        serde_json::from_value(input).unwrap(),
        facts,
        &mut ids,
        &policy(Dialect::OpenAi),
    )
    .unwrap()
    .value;
    assert_eq!(
        output
            .usage
            .unwrap()
            .unwrap()
            .input_tokens_details
            .cached_tokens,
        3
    );
    let mut input = chat();
    input["usage"]["total_tokens"] = json!(18);
    assert!(
        chat_to_responses_response(
            serde_json::from_value(input).unwrap(),
            context(),
            &mut flow(11),
            &policy(Dialect::OpenAi)
        )
        .is_ok()
    );
}
#[test]
fn annotations_logprobs_and_multiple_refusals_are_preserved() {
    let mut input = chat();
    input["choices"][0]["message"]["annotations"] = json!([{"type":"url_citation","url_citation":{"start_index":0,"end_index":5,"title":"Source","url":"https://example.test","future":1},"future":2}]);
    input["choices"][0]["logprobs"] = json!({"content":[{"token":"hello","bytes":[104,101,108,108,111],"logprob":-0.2,"top_logprobs":[],"future":1}],"refusal":null});
    let output = convert(input, 12);
    let mut value = serde_json::to_value(output).unwrap();
    assert_eq!(
        value["output"][0]["content"][0]["annotations"][0]["url"],
        "https://example.test"
    );
    assert_eq!(
        value["output"][0]["content"][0]["logprobs"][0]["logprob"],
        -0.2
    );
    value["output"][0]["content"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"refusal","refusal":" again"}));
    let back = responses_to_chat_response(
        serde_json::from_value(value).unwrap(),
        &mut flow(13),
        &policy(Dialect::OpenAiChat),
    )
    .unwrap()
    .value;
    assert_eq!(
        back.choices[0].message.refusal.as_deref(),
        Some("also refused again")
    );
    assert_eq!(
        back.choices[0].message.annotations.as_ref().unwrap()[0]
            .url_citation
            .start_index,
        0
    );
    assert_eq!(
        back.choices[0]
            .logprobs
            .as_ref()
            .unwrap()
            .content
            .as_ref()
            .unwrap()[0]
            .logprob,
        -0.2
    );
}
#[test]
fn legacy_function_response_gets_distinct_stable_call_and_item_ids() {
    let mut input = chat();
    input["choices"][0]["finish_reason"] = json!("function_call");
    input["choices"][0]["message"]["function_call"] = json!({"name":"lookup","arguments":"broken"});
    let first = serde_json::to_value(convert(input.clone(), 14)).unwrap();
    let second = serde_json::to_value(convert(input, 14)).unwrap();
    assert_eq!(first, second);
    assert_ne!(first["output"][1]["id"], first["output"][1]["call_id"]);
    assert_eq!(first["output"][1]["arguments"], "broken");
}

#[test]
fn a_late_context_failure_does_not_publish_partial_identity_state() {
    let mut ids = flow(20);
    let mut wrong = context();
    wrong.effective_parallel_tool_calls = true;
    let result = chat_to_responses_response(
        serde_json::from_value(chat()).unwrap(),
        wrong,
        &mut ids,
        &policy(Dialect::OpenAi),
    );
    assert!(result.is_err());
    assert!(
        ids.lookup_emitted_as(IdentityRole::Response, "chatcmpl_native")
            .is_none()
    );
    let mut input = chat();
    input["id"] = json!("chatcmpl_after_error");
    let retried = chat_to_responses_response(
        serde_json::from_value(input.clone()).unwrap(),
        context(),
        &mut ids,
        &policy(Dialect::OpenAi),
    )
    .unwrap()
    .value;
    let fresh = convert(input, 20);
    assert_eq!(retried, fresh);
}

#[test]
fn moderation_outcomes_preserve_scores_and_typed_errors() {
    let mut input = chat();
    input["moderation"] = json!({"input":{"type":"moderation_results","model":"moderation-model","results":[{"type":"moderation_result","model":"moderation-model","flagged":true,"categories":{"unsafe":true},"category_scores":{"unsafe":0.75},"category_applied_input_types":{"unsafe":["text","image"]},"future":1}],"future":2},"output":{"type":"error","code":"timeout","message":"unavailable","future":3},"future":4});
    let output = convert(input.clone(), 21);
    let value = serde_json::to_value(&output).unwrap();
    assert_eq!(
        value["moderation"]["input"]["category_scores"]["unsafe"],
        0.75
    );
    assert_eq!(value["moderation"]["output"]["code"], "timeout");
    let back = responses_to_chat_response(output, &mut flow(22), &policy(Dialect::OpenAiChat))
        .unwrap()
        .value;
    let value = serde_json::to_value(back).unwrap();
    assert_eq!(
        value["moderation"]["input"]["results"][0]["category_applied_input_types"]["unsafe"],
        json!(["text", "image"])
    );
    assert!(value["moderation"]["input"].get("future").is_none());
}

#[test]
fn duplicate_tool_identity_and_usage_conflicts_are_errors() {
    let mut input = chat();
    input["choices"][0]["message"]["tool_calls"] = json!([
        {"type":"function","id":"same","function":{"name":"one","arguments":"{}"}},
        {"type":"function","id":"same","function":{"name":"two","arguments":"{}"}}
    ]);
    assert_eq!(
        chat_to_responses_response(
            serde_json::from_value(input).unwrap(),
            context(),
            &mut flow(23),
            &policy(Dialect::OpenAi)
        )
        .unwrap_err()
        .kind(),
        TransformErrorKind::Conflict
    );
    let mut facts = context();
    facts.usage.cached_tokens = Some(99);
    assert!(
        chat_to_responses_response(
            serde_json::from_value(chat()).unwrap(),
            facts,
            &mut flow(24),
            &policy(Dialect::OpenAi)
        )
        .is_ok()
    );
}

#[test]
fn one_choice_with_nonzero_index_is_invalid() {
    let mut value = chat();
    value["choices"][0]["index"] = json!(2);
    let error = chat_to_responses_response(
        serde_json::from_value(value).unwrap(),
        context(),
        &mut flow(77),
        &policy(Dialect::OpenAi),
    )
    .unwrap_err();
    assert_eq!(error.context(), "choices.index");
}

#[test]
fn negative_chat_annotation_range_is_invalid() {
    let mut value = chat();
    value["choices"][0]["message"]["annotations"] = json!([{"type":"url_citation","url_citation":{"start_index":-1,"end_index":2,"title":"x","url":"https://example"}}]);
    assert!(
        chat_to_responses_response(
            serde_json::from_value(value).unwrap(),
            context(),
            &mut flow(78),
            &policy(Dialect::OpenAi)
        )
        .is_err()
    );
}
