use serde_json::{Value, json};

use super::*;
use crate::protocol::openai::{IncompleteReason, ResponseErrorCode, ResponseErrorCodeKnown};

#[test]
fn async_tools_calls_and_configuration_updates_round_trip() {
    let wire = json!({
        "model":"gpt-6-astra",
        "tools":[
            {"type":"function","name":"lookup","parameters":{},"async":true},
            {"type":"custom","name":"render","async":true},
            {"type":"namespace","name":"jobs","description":"background jobs","tools":[
                {"type":"function","name":"poll","async":true}
            ]}
        ],
        "input":[{"type":"configuration_update","reasoning":{"effort":"high"}}]
    });
    let request: ResponseCreateRequest = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(request).unwrap(), wire);

    for call in [
        json!({
            "type":"function_call","call_id":"call_1","name":"lookup",
            "arguments":"{}","async":true
        }),
        json!({
            "type":"custom_tool_call","call_id":"call_2","name":"render",
            "input":"page","async":true
        }),
    ] {
        let item: ResponseItem = serde_json::from_value(call.clone()).unwrap();
        assert_eq!(serde_json::to_value(item).unwrap(), call);
    }
}

#[test]
fn steering_requests_and_events_round_trip() {
    let request = json!({
        "type":"response.steer","previous_response_id":"resp_1",
        "input":"Keep the migration focused."
    });
    let parsed: ResponseWebSocketRequest = serde_json::from_value(request.clone()).unwrap();
    assert!(matches!(parsed, ResponseWebSocketRequest::ResponseSteer(_)));
    assert_eq!(serde_json::to_value(parsed).unwrap(), request);

    for event in [
        json!({
            "type":"response.steer.accepted","sequence_number":4,
            "steer":{"id":"steer_1","previous_response_id":"resp_1"}
        }),
        json!({
            "type":"response.steer.pending","sequence_number":5,
            "steer":{"id":"steer_1","previous_response_id":"resp_1"},
            "reason":"waiting_for_required_input","required_input":[
                {"type":"function_call_output","call_id":"call_1","name":"lookup"}
            ]
        }),
        json!({
            "type":"response.steer.failed","sequence_number":6,
            "steer":{"id":"steer_1","previous_response_id":"resp_1","input":"focus"},
            "error":{"type":"invalid_request_error","code":"steering_not_supported","message":"unsupported"}
        }),
    ] {
        let parsed: ResponseStreamEvent = serde_json::from_value(event.clone()).unwrap();
        assert!(matches!(parsed, ResponseStreamEvent::Known(_)));
        assert_eq!(serde_json::to_value(parsed).unwrap(), event);
    }

    let details: IncompleteDetails = serde_json::from_value(json!({"reason":"steered"})).unwrap();
    assert_eq!(details.reason, Some(IncompleteReason::Steered));
    let code: ResponseErrorCode =
        serde_json::from_value(Value::String("misalignment_policy_violation".into())).unwrap();
    assert_eq!(
        code,
        ResponseErrorCode::Known(ResponseErrorCodeKnown::MisalignmentPolicyViolation)
    );
}
