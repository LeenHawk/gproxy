use serde_json::json;

use crate::openai::generate_content::responses::{ResponseCreateRequest, ResponseObject};

use super::round_trip;

#[test]
fn multi_agent_config_round_trips_request_response_and_absence() {
    let request = round_trip::<ResponseCreateRequest>(json!({
        "multi_agent": {
            "enabled": true,
            "max_concurrent_subagents": 3,
            "future_multi_agent": {"mode": "tree"}
        },
        "request_future": true
    }));
    let config = request.multi_agent.expect("request multi-agent config");
    assert!(config.enabled);
    assert_eq!(config.max_concurrent_subagents, Some(3));
    assert_eq!(config.rest["future_multi_agent"]["mode"], "tree");

    let response = round_trip::<ResponseObject>(json!({
        "id": "resp_1",
        "object": "response",
        "output": [],
        "multi_agent": {
            "enabled": true,
            "max_concurrent_subagents": 3,
            "future_multi_agent": {"mode": "tree"}
        }
    }));
    assert_eq!(response.multi_agent, Some(config));

    let request = round_trip::<ResponseCreateRequest>(json!({}));
    assert!(request.multi_agent.is_none());
    let response = round_trip::<ResponseObject>(json!({
        "id": "resp_2",
        "object": "response",
        "output": []
    }));
    assert!(response.multi_agent.is_none());
}
