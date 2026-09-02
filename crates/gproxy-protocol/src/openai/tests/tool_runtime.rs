use serde_json::json;

use crate::openai::common::{ResponseAllowedTool, ResponseAllowedToolChoice, ResponseToolChoice};
use crate::openai::generate_content::responses::{
    CodeInterpreterContainer, CodeInterpreterNetworkPolicy, ResponseCreateRequest,
    ResponseShellContainerSkill, ResponseShellEnvironment, ResponseTool,
};

use super::round_trip;

#[test]
fn response_tool_runtime_unions_round_trip() {
    let parsed = round_trip::<ResponseCreateRequest>(json!({
        "tools": [
            {
                "type": "code_interpreter",
                "container": {
                    "type": "auto",
                    "network_policy": {
                        "type": "allowlist",
                        "allowed_domains": ["example.com"],
                        "domain_secrets": [{
                            "domain": "example.com",
                            "name": "TEST_SETTING",
                            "value": "value",
                            "secret_future": true
                        }],
                        "policy_future": 1
                    },
                    "container_future": true
                },
                "interpreter_future": true
            },
            {
                "type": "shell",
                "environment": {
                    "type": "container_auto",
                    "network_policy": {"type": "disabled", "policy_future": 2},
                    "skills": [
                        {
                            "type": "skill_reference",
                            "skill_id": "skill_1",
                            "version": "latest",
                            "reference_future": true
                        },
                        {
                            "type": "inline",
                            "name": "review",
                            "description": "Review changes",
                            "source": {
                                "type": "base64",
                                "media_type": "application/zip",
                                "data": "eA==",
                                "source_future": true
                            },
                            "inline_future": true
                        }
                    ],
                    "environment_future": true
                },
                "shell_future": true
            },
            {
                "type": "shell",
                "environment": {
                    "type": "local",
                    "skills": [{
                        "name": "local-review",
                        "description": "Review locally",
                        "path": "/skills/review",
                        "local_skill_future": true
                    }],
                    "local_future": true
                }
            },
            {
                "type": "shell",
                "environment": {
                    "type": "container_reference",
                    "container_id": "cntr_1",
                    "reference_future": true
                }
            }
        ]
    }));
    let tools = parsed.tools.expect("runtime tools");
    assert!(matches!(
        &tools[0],
        ResponseTool::CodeInterpreter {
            container: CodeInterpreterContainer::Auto(container),
            ..
        } if matches!(
            &container.network_policy,
            Some(CodeInterpreterNetworkPolicy::Allowlist { .. })
        )
    ));
    let ResponseTool::Shell {
        environment:
            Some(ResponseShellEnvironment::ContainerAuto {
                network_policy: Some(CodeInterpreterNetworkPolicy::Disabled { .. }),
                skills: Some(skills),
                ..
            }),
        ..
    } = &tools[1]
    else {
        panic!("expected container_auto shell environment");
    };
    assert!(matches!(
        skills.as_slice(),
        [
            ResponseShellContainerSkill::Reference(_),
            ResponseShellContainerSkill::Inline(_)
        ]
    ));
    assert!(matches!(
        &tools[2],
        ResponseTool::Shell {
            environment: Some(ResponseShellEnvironment::Local { .. }),
            ..
        }
    ));
    assert!(matches!(
        &tools[3],
        ResponseTool::Shell {
            environment: Some(ResponseShellEnvironment::ContainerReference { .. }),
            ..
        }
    ));

    let allowed = round_trip::<ResponseAllowedToolChoice>(json!({
        "type":"allowed_tools",
        "mode":"required",
        "tools":[
            {"type":"function","name":"f","future":1},
            {"type":"custom","name":"c","future":2},
            {"type":"mcp","server_label":"srv","name":"lookup","future":3},
            {"type":"file_search","future":4},
            {"type":"web_search_preview","future":5},
            {"type":"computer","future":6},
            {"type":"computer_use_preview","future":7},
            {"type":"computer_use","future":8},
            {"type":"web_search_preview_2025_03_11","future":9},
            {"type":"image_generation","future":10},
            {"type":"code_interpreter","future":11},
            {"type":"local_shell","future":12},
            {"type":"shell","future":13},
            {"type":"apply_patch","future":14}
        ],
        "choice_future":true
    }));
    assert!(matches!(
        &allowed.tools[0],
        ResponseAllowedTool::Function { name, rest }
            if name == "f" && rest["future"] == 1
    ));
    assert!(matches!(
        &allowed.tools[2],
        ResponseAllowedTool::Mcp {
            server_label,
            name: Some(name),
            rest,
        } if server_label == "srv" && name == "lookup" && rest["future"] == 3
    ));
}

#[test]
fn named_response_tool_choice_is_not_consumed_by_the_hosted_fallback() {
    let choice = round_trip::<ResponseToolChoice>(json!({
        "type":"function",
        "name":"get_weather"
    }));
    assert!(matches!(
        choice,
        ResponseToolChoice::Function(choice) if choice.name == "get_weather"
    ));
}
