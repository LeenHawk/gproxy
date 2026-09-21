//! Native nullable fields from Create a model response.md:15512-15515 and
//! Responses.md:33257-33260. The fixture supplies current required sequence IDs.
use gproxy_protocol::{
    transform::{
        generate::{
            chat_responses::responses_to_chat_response, stream::responses::ResponsesStreamCollector,
        },
        identity::{IdNamespace, IdentityFlow, TargetIdPolicy},
    },
    wire::{
        DeclaredFields,
        openai::responses::{response as r, stream as s},
    },
};
use serde_json::{Value, json};
fn initial() -> Vec<Value> {
    serde_json::from_str(include_str!("fixtures/responses_nullable_frames.json")).unwrap()
}
#[test]
fn native_created_and_in_progress_null_fields_decode_and_roundtrip() {
    for value in initial() {
        let event: s::StreamEvent = serde_json::from_value(value.clone()).unwrap();
        let body = match &event {
            s::StreamEvent::Created(v) => &v.response,
            s::StreamEvent::InProgress(v) => &v.response,
            _ => panic!(),
        };
        assert!(matches!(body.usage, Some(None)));
        assert!(matches!(body.user, Some(None)));
        assert_eq!(serde_json::to_value(&event).unwrap(), value);
        let declared = serde_json::to_value(event.into_declared()).unwrap();
        assert_eq!(declared["response"]["usage"], Value::Null);
        assert_eq!(declared["response"]["user"], Value::Null);
        // The example's undeclared store response extension stays a wire-only
        // extension, rather than becoming a newly promoted schema field.
        assert!(declared["response"].get("store").is_none());
    }
}
#[test]
fn absent_null_and_present_usage_and_user_remain_distinct() {
    let mut value = initial()[0]["response"].clone();
    value.as_object_mut().unwrap().remove("usage");
    value.as_object_mut().unwrap().remove("user");
    let absent: r::GenerateContentResponseBody = serde_json::from_value(value.clone()).unwrap();
    assert!(absent.usage.is_none());
    assert!(absent.user.is_none());
    let encoded = serde_json::to_value(absent).unwrap();
    assert!(encoded.get("usage").is_none());
    assert!(encoded.get("user").is_none());
    value["usage"] = Value::Null;
    value["user"] = Value::Null;
    let null: r::GenerateContentResponseBody = serde_json::from_value(value.clone()).unwrap();
    assert!(matches!(null.usage, Some(None)));
    assert!(matches!(null.user, Some(None)));
    assert_eq!(serde_json::to_value(null).unwrap(), value);
    value["usage"] = json!({"input_tokens":5,"output_tokens":7,"total_tokens":12,"input_tokens_details":{"cached_tokens":1,"cache_write_tokens":0},"output_tokens_details":{"reasoning_tokens":2}});
    value["user"] = json!("actual-user");
    let present: r::GenerateContentResponseBody = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(
        present
            .usage
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .total_tokens,
        12
    );
    assert_eq!(
        present.user.as_ref().unwrap().as_deref(),
        Some("actual-user")
    );
    assert_eq!(serde_json::to_value(present).unwrap(), value);
}
#[test]
fn collector_preserves_unknown_null_usage_without_fabricated_tokens() {
    let mut frames = initial();
    let mut terminal = frames[0]["response"].clone();
    terminal["status"] = json!("completed");
    frames.push(json!({"type":"response.completed","sequence_number":2,"response":terminal}));
    let mut collector = ResponsesStreamCollector::new(Default::default());
    for frame in frames {
        collector
            .push(serde_json::from_value(frame).unwrap())
            .unwrap();
    }
    let response = collector.finish().unwrap().value;
    assert!(matches!(response.usage, Some(None)));
    assert!(matches!(response.user, Some(None)));
    let value = serde_json::to_value(&response).unwrap();
    assert_eq!(value["usage"], Value::Null);
    assert_eq!(value["user"], Value::Null);
    let mut flow = IdentityFlow::new(IdNamespace::with_bytes([9; 16]));
    let mapped = responses_to_chat_response(
        response,
        &mut flow,
        &TargetIdPolicy::new(gproxy_protocol::Dialect::OpenAiChat),
    )
    .unwrap()
    .value;
    assert!(mapped.usage.is_none());
}
#[test]
fn required_usage_detail_fields_are_not_weakened() {
    let mut value = initial()[0]["response"].clone();
    value["usage"] = json!({"input_tokens":5,"output_tokens":7,"total_tokens":12,"input_tokens_details":{"cached_tokens":1},"output_tokens_details":{"reasoning_tokens":2}});
    assert!(serde_json::from_value::<r::GenerateContentResponseBody>(value).is_err());
}
