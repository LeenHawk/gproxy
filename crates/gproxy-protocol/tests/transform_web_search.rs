use gproxy_protocol::transform::{
    generate::{claude_responses, gemini_responses},
    web_search,
};
use serde_json::{Value, json};

#[test]
fn browser_commands_preserve_context_without_requiring_a_search_query() {
    let commands = json!({
        "open":[{"ref_id":"https://example.com/docs", "lineno":20}],
        "find":[{"ref_id":"page1", "pattern":"install"}],
        "click":[{"ref_id":"page1", "id":3}],
        "response_length":"long"
    });
    let context = "page1 is https://example.com/docs; link 3 is https://example.com/install";
    let request = web_search::to_generation(
        serde_json::from_value(json!({
            "id":"session", "model":"client", "commands":commands,
            "input":context, "max_output_tokens":1024
        }))
        .unwrap(),
        "selected".into(),
    )
    .unwrap();
    let wire = serde_json::to_value(&request).unwrap();
    let payload: Value = serde_json::from_str(wire["input"].as_str().unwrap()).unwrap();
    assert_eq!(payload["commands"], commands);
    assert_eq!(payload["input"], context);
    assert_eq!(wire["max_output_tokens"], 1024);

    let claude = claude_responses::responses_to_claude_request(
        request.clone(),
        "selected",
        Default::default(),
    )
    .unwrap()
    .value;
    let claude = serde_json::to_value(claude).unwrap();
    assert!(
        claude["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "web_fetch")
    );
    let gemini =
        gemini_responses::responses_to_gemini_request(request, "selected", Default::default())
            .unwrap()
            .value;
    let gemini = serde_json::to_value(gemini).unwrap();
    assert!(
        gemini["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool.get("urlContext").is_some())
    );
}

#[test]
fn missing_or_empty_commands_and_nontext_results_are_not_errors() {
    for commands in [
        Value::Null,
        json!({}),
        json!({"search_query":[]}),
        json!({"screenshot":[{"ref_id":"page1","pageno":1}]}),
    ] {
        web_search::to_generation(
            serde_json::from_value(json!({
                "id":"session", "model":"client", "commands":commands, "input":"Find documentation"
            }))
            .unwrap(),
            "selected".into(),
        )
        .unwrap();
    }
    for output in [
        json!([]),
        json!([{"type":"reasoning", "id":"rs_1", "summary":[]}]),
    ] {
        let response = serde_json::from_value(json!({
            "id":"resp_1", "created_at":0, "error":null, "incomplete_details":null,
            "instructions":null, "metadata":null, "model":"selected", "object":"response",
            "output":output, "parallel_tool_calls":true, "temperature":null,
            "tool_choice":"auto", "tools":[], "top_p":null
        }))
        .unwrap();
        assert_eq!(web_search::from_generation(response).unwrap().output, "");
    }
}
