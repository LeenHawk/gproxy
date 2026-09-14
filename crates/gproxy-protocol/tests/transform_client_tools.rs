#[path = "support/client_tools.rs"]
mod fixtures;
use gproxy_protocol::{
    Dialect,
    transform::{generate::chat_responses::*, identity::*},
    wire::openai::responses as r,
};
use serde_json::json;
fn flow() -> IdentityFlow {
    IdentityFlow::new(IdNamespace([93; 16]))
}
fn policy() -> TargetIdPolicy {
    TargetIdPolicy::new(Dialect::OpenAi).with_syntax(IdSyntax::AsciiIdentifier)
}

#[test]
fn local_tools_namespace_and_discovery_complete_a_buffered_round_trip() {
    let original: r::GenerateContentRequestBody =
        serde_json::from_value(fixtures::request(false)).unwrap();
    let target = responses_to_chat_request(original.clone(), "selected")
        .unwrap()
        .value;
    let wire = serde_json::to_value(&target).unwrap();
    let names: Vec<_> = wire["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["function"]["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        names.len(),
        4,
        "undiscovered deferred tool must remain unavailable"
    );
    let converted = chat_to_responses_response(
        serde_json::from_value(fixtures::response(&names)).unwrap(),
        ResponsesResponseContext {
            request: original,
            effective_parallel_tool_calls: true,
            effective_tool_choice: r::ToolChoice::Mode(r::ToolChoiceMode::Auto),
            usage: Default::default(),
            effective_prompt_cache_options: None,
        },
        &mut flow(),
        &policy(),
    )
    .unwrap()
    .value;
    let wire = serde_json::to_value(converted).unwrap();
    let kinds: Vec<_> = wire["output"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["type"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "shell_call",
            "apply_patch_call",
            "function_call",
            "tool_search_call"
        ]
    );
    assert_eq!(wire["output"][0]["action"]["timeout_ms"], 1000);
    assert_eq!(wire["output"][1]["operation"]["diff"], "@@\n-old\n+new");
    assert_eq!(wire["output"][2]["name"], "lookup");
    assert_eq!(wire["output"][2]["namespace"], "repo");
    assert_eq!(wire["output"][3]["execution"], "client");
    for item in wire["output"].as_array().unwrap() {
        assert_ne!(item["id"], item["call_id"]);
    }
    let next = responses_to_chat_request(
        serde_json::from_value(fixtures::followup(&wire)).unwrap(),
        "selected",
    )
    .unwrap()
    .value;
    let next = serde_json::to_value(next).unwrap();
    assert_eq!(next["tools"].as_array().unwrap().len(), 5);
    assert!(next["messages"].to_string().contains("context mismatch"));
    assert!(next["messages"].to_string().contains("exit_code"));
}

#[test]
fn execution_scopes_and_alias_collisions_are_not_silently_downgraded() {
    for tool in [
        json!({"type":"shell","environment":{"type":"container_auto"}}),
        json!({"type":"apply_patch","allowed_callers":["programmatic"]}),
        json!({"type":"tool_search","execution":"server"}),
        json!({"type":"namespace","name":"repo","description":"x","tools":[{"type":"function","name":"f","allowed_callers":["programmatic"]}]}),
    ] {
        let input =
            serde_json::from_value(json!({"model":"source","input":"hi","tools":[tool]})).unwrap();
        assert!(responses_to_chat_request(input, "target").is_err());
    }
    let input = serde_json::from_value(json!({"model":"source","input":"hi","tools":[{"type":"shell"},{"type":"function","name":"gproxy_client_shell","parameters":{},"strict":false}]})).unwrap();
    assert!(responses_to_chat_request(input, "target").is_err());
}

#[test]
fn namespaces_do_not_collapse_same_named_functions() {
    let mut input = fixtures::request(false);
    let mut second = input["tools"][2].clone();
    second["name"] = json!("other");
    input["tools"].as_array_mut().unwrap().push(second);
    let output =
        responses_to_chat_request(serde_json::from_value(input).unwrap(), "target").unwrap();
    let wire = serde_json::to_value(output.value).unwrap();
    assert_ne!(
        wire["tools"][2]["function"]["name"],
        wire["tools"][4]["function"]["name"]
    );
}

#[test]
fn selected_tools_and_discovered_alias_collisions_keep_their_constraints() {
    let mut source = fixtures::request(false);
    source["tool_choice"] = json!({"type":"allowed_tools","mode":"required","tools":[{"type":"shell"},{"type":"function","namespace":"repo","name":"lookup"}]});
    let output = responses_to_chat_request(serde_json::from_value(source).unwrap(), "target")
        .unwrap()
        .value;
    let wire = serde_json::to_value(output).unwrap();
    assert_eq!(wire["tool_choice"]["allowed_tools"]["mode"], "required");
    assert_eq!(
        wire["tool_choice"]["allowed_tools"]["tools"][0]["function"]["name"],
        "gproxy_client_shell"
    );
    assert_eq!(
        wire["tool_choice"]["allowed_tools"]["tools"][1]["function"]["name"],
        wire["tools"][2]["function"]["name"]
    );
    let mut source = fixtures::request(false);
    source["input"] = json!([{"type":"additional_tools","role":"developer","tools":[{"type":"function","name":"gproxy_client_shell","parameters":{"type":"object"},"strict":false}]}]);
    assert!(responses_to_chat_request(serde_json::from_value(source).unwrap(), "target").is_err());
}
