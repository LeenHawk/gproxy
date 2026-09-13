use gproxy_protocol::openai::responses::tools::*;

fn round_trip(value: serde_json::Value) -> serde_json::Value {
    serde_json::to_value(serde_json::from_value::<Tool>(value).unwrap()).unwrap()
}

#[test]
fn filters_have_typed_operator_and_mixed_in_values() {
    let value = serde_json::json!({"type":"file_search","vector_store_ids":["vs"],"filters":{"type":"in","key":"status","value":["new",1]},"future":7});
    let tool: Tool = serde_json::from_value(value).unwrap();
    let Tool::FileSearch(file) = tool else {
        panic!()
    };
    let Some(Some(FileSearchFilter::Comparison(filter))) = file.filters else {
        panic!()
    };
    assert!(matches!(filter.type_, ComparisonFilterType::In));
    assert!(file.rest.contains_key("future"));
    let invalid = serde_json::json!({"type":"file_search","vector_store_ids":["vs"],"filters":{"type":"in","key":"status","value":[true]}});
    assert!(serde_json::from_value::<Tool>(invalid).is_err());
}

#[test]
fn compound_filter_and_or_are_recursive_and_unknown_is_rest() {
    let value = serde_json::json!({"type":"file_search","vector_store_ids":["vs"],"filters":{"type":"and","filters":[{"type":"eq","key":"a","value":1},{"type":"or","filters":[{"type":"ne","key":"b","value":false}],"nested_future":true}]}});
    let Tool::FileSearch(file) = serde_json::from_value::<Tool>(value).unwrap() else {
        panic!()
    };
    assert!(matches!(
        file.filters,
        Some(Some(FileSearchFilter::Compound(_)))
    ));
}

#[test]
fn shell_supports_all_three_typed_environments() {
    for (env, expected) in [
        (
            serde_json::json!({"type":"container_auto","memory_limit":"1g"}),
            "auto",
        ),
        (
            serde_json::json!({"type":"local","skills":[{"name":"s","description":"d","path":"/s"}]}),
            "local",
        ),
        (
            serde_json::json!({"type":"container_reference","container_id":"c"}),
            "reference",
        ),
    ] {
        let value =
            serde_json::json!({"type":"shell","environment":env,"allowed_callers":["direct"]});
        let Tool::Shell(shell) = serde_json::from_value::<Tool>(value).unwrap() else {
            panic!()
        };
        assert!(matches!(
            (expected, shell.environment),
            ("auto", Some(Some(ShellEnvironment::Auto(_))))
                | ("local", Some(Some(ShellEnvironment::Local(_))))
                | ("reference", Some(Some(ShellEnvironment::Reference(_))))
        ));
    }
}

#[test]
fn code_interpreter_auto_container_and_memory_limits_are_typed() {
    let value = serde_json::json!({"type":"code_interpreter","container":{"type":"auto","file_ids":["f"],"memory_limit":"64g","network_policy":{"type":"disabled"}},"allowed_callers":["programmatic"]});
    let Tool::CodeInterpreter(tool) = serde_json::from_value(value).unwrap() else {
        panic!()
    };
    assert!(
        matches!(tool.container, CodeInterpreterContainer::Auto(auto) if matches!(auto.memory_limit, Some(Some(MemoryLimit::SixtyFourG))))
    );
}

#[test]
fn mcp_allowed_tools_and_approval_support_array_and_filter_forms() {
    for (allowed_tools, approval) in [
        (serde_json::json!(["read"]), serde_json::json!("never")),
        (
            serde_json::json!({"read_only":true,"tool_names":["write"]}),
            serde_json::json!({"always":{"read_only":false}}),
        ),
    ] {
        let value = serde_json::json!({"type":"mcp","server_label":"srv","allowed_tools":allowed_tools,"require_approval":approval,"headers":{"x":"y"},"connector_id":"connector_gmail","future":1});
        let Tool::Mcp(tool) = serde_json::from_value::<Tool>(value).unwrap() else {
            panic!()
        };
        assert!(tool.rest.contains_key("future"));
        assert!(tool.headers.is_some());
    }
}

#[test]
fn web_search_preview_long_tag_and_location_are_preserved() {
    let value = serde_json::json!({"type":"web_search_preview_2025_03_11","search_context_size":"high","search_content_types":["text","image"],"user_location":{"type":"approximate","country":"US"}});
    let encoded = round_trip(value.clone());
    assert_eq!(encoded, value);
}

#[test]
fn image_mask_and_custom_grammar_are_typed() {
    let image = serde_json::json!({"type":"image_generation","input_fidelity":"high","input_image_mask":{"image_url":"https://x"},"moderation":"low","model":"any-model","size":"any-size"});
    let Tool::ImageGeneration(image) = serde_json::from_value::<Tool>(image).unwrap() else {
        panic!()
    };
    assert!(matches!(
        image.input_image_mask,
        Some(InputImageMask {
            image_url: Some(_),
            ..
        })
    ));
    let custom = serde_json::json!({"type":"custom","name":"parse","format":{"type":"grammar","definition":"start: WORD","syntax":"lark"}});
    let Tool::Custom(custom) = serde_json::from_value::<Tool>(custom).unwrap() else {
        panic!()
    };
    assert!(matches!(custom.format, Some(CustomToolFormat::Grammar(_))));
}

#[test]
fn namespace_function_and_custom_keep_distinct_optional_fields() {
    let value = serde_json::json!({"type":"namespace","name":"crm","description":"CRM","tools":[{"name":"lookup","type":"function","parameters":null,"strict":null},{"name":"emit","type":"custom","format":{"type":"text"}}]});
    let Tool::Namespace(namespace) = serde_json::from_value::<Tool>(value).unwrap() else {
        panic!()
    };
    assert_eq!(namespace.tools.len(), 2);
    assert!(matches!(
        namespace.tools[0],
        NamespaceToolDefinition::Function(_)
    ));
    assert!(matches!(
        namespace.tools[1],
        NamespaceToolDefinition::Custom(_)
    ));
}

#[test]
fn documented_nullable_fields_preserve_missing_null_and_value() {
    use serde_json::{Value, json};

    let cases: Vec<(Value, &[&str])> = vec![
        (
            json!({"type":"function","name":"f","parameters":{},"strict":false,"description":"d","allowed_callers":["direct"],"output_schema":{}}),
            &["/description", "/allowed_callers", "/output_schema"],
        ),
        (
            json!({"type":"file_search","vector_store_ids":[],"filters":{"type":"eq","key":"a","value":1}}),
            &["/filters"],
        ),
        (
            json!({"type":"web_search","filters":{"allowed_domains":["example.com"]},"user_location":{"city":"c","country":"US","region":"r","timezone":"UTC"}}),
            &[
                "/filters",
                "/filters/allowed_domains",
                "/user_location",
                "/user_location/city",
                "/user_location/country",
                "/user_location/region",
                "/user_location/timezone",
            ],
        ),
        (
            json!({"type":"web_search_preview","user_location":{"type":"approximate","city":"c","country":"US","region":"r","timezone":"UTC"}}),
            &[
                "/user_location",
                "/user_location/city",
                "/user_location/country",
                "/user_location/region",
                "/user_location/timezone",
            ],
        ),
        (
            json!({"type":"code_interpreter","container":{"type":"auto","memory_limit":"4g"},"allowed_callers":["programmatic"]}),
            &["/container/memory_limit", "/allowed_callers"],
        ),
        (
            json!({"type":"custom","name":"c","allowed_callers":["direct"]}),
            &["/allowed_callers"],
        ),
        (
            json!({"type":"apply_patch","allowed_callers":["direct"]}),
            &["/allowed_callers"],
        ),
        (
            json!({"type":"shell","allowed_callers":["direct"],"environment":{"type":"container_auto","memory_limit":"16g"}}),
            &[
                "/allowed_callers",
                "/environment",
                "/environment/memory_limit",
            ],
        ),
        (
            json!({"type":"image_generation","input_fidelity":"high"}),
            &["/input_fidelity"],
        ),
        (
            json!({"type":"mcp","server_label":"m","allowed_callers":["direct"],"allowed_tools":["f"],"headers":{"x":"y"},"require_approval":"never"}),
            &[
                "/allowed_callers",
                "/allowed_tools",
                "/headers",
                "/require_approval",
            ],
        ),
        (
            json!({"type":"tool_search","description":"d","parameters":[1,"arbitrary",true]}),
            &["/description", "/parameters"],
        ),
        (
            json!({"type":"namespace","name":"n","description":"d","tools":[{"type":"function","name":"f","allowed_callers":["direct"],"description":"d","output_schema":{},"parameters":[1],"strict":false},{"type":"custom","name":"c","allowed_callers":["programmatic"]}]}),
            &[
                "/tools/0/allowed_callers",
                "/tools/0/description",
                "/tools/0/output_schema",
                "/tools/0/parameters",
                "/tools/0/strict",
                "/tools/1/allowed_callers",
            ],
        ),
    ];
    for (value, paths) in cases {
        assert_eq!(round_trip(value.clone()), value);
        for path in paths {
            let (parent, key) = path.rsplit_once('/').unwrap();
            let mut missing = value.clone();
            missing
                .pointer_mut(parent)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(key);
            assert_eq!(round_trip(missing.clone()), missing, "missing {path}");
            let mut null = value.clone();
            *null.pointer_mut(path).unwrap() = Value::Null;
            assert_eq!(round_trip(null.clone()), null, "null {path}");
        }
    }
}

#[test]
fn required_nullable_function_fields_remain_required_in_wire_and_builder() {
    use serde_json::json;

    for (parameters, strict) in [(json!(null), json!(null)), (json!({}), json!(false))] {
        let value = json!({"type":"function","name":"f","parameters":parameters,"strict":strict});
        assert_eq!(round_trip(value.clone()), value);
        for key in ["parameters", "strict"] {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(key);
            assert!(
                serde_json::from_value::<Tool>(missing).is_err(),
                "missing {key}"
            );
        }
    }
    let tool = Tool::Function(FunctionTool::builder("f".into(), None, None).build());
    assert_eq!(
        serde_json::to_value(tool).unwrap(),
        json!({"type":"function","name":"f","parameters":null,"strict":null})
    );
    let tool = Tool::Function(
        FunctionTool::builder("f".into(), None, None)
            .description(None)
            .build(),
    );
    assert_eq!(
        serde_json::to_value(tool).unwrap(),
        json!({"type":"function","name":"f","parameters":null,"strict":null,"description":null})
    );
}

#[test]
fn computer_environment_and_allowed_callers_reject_unknown_values() {
    for value in [
        serde_json::json!({"type":"computer_use_preview","display_height":1,"display_width":1,"environment":"anything"}),
        serde_json::json!({"type":"function","name":"f","parameters":{},"strict":false,"allowed_callers":["anything"]}),
        serde_json::json!({"type":"custom","name":"c","allowed_callers":["anything"]}),
    ] {
        assert!(serde_json::from_value::<Tool>(value).is_err());
    }
}
