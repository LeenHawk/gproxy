use gproxy_protocol::openai::responses::input::*;

#[test]
fn web_search_and_annotations_use_fixed_nested_tags() {
    let action: WebSearchAction = serde_json::from_value(serde_json::json!({
        "type":"search", "sources":[{"type":"url","url":"https://example.com"}]
    }))
    .unwrap();
    match action {
        WebSearchAction::Search(v) => assert!(v.rest.is_empty()),
        _ => panic!(),
    }
    let text: ResponseOutputText = serde_json::from_value(serde_json::json!({
        "type":"output_text","text":"x","annotations":[{"type":"file_path","file_id":"f","index":1}],"logprobs":[]
    })).unwrap();
    assert!(text.rest.is_empty());
    assert!(matches!(text.annotations[0], OutputAnnotation::Path(_)));
}

#[test]
fn mcp_schema_and_nullable_output_content_round_trip() {
    let item: McpToolDefinition = serde_json::from_value(serde_json::json!({
        "name":"read","input_schema":{"type":"object"},"annotations":null,"description":null,"future":true
    })).unwrap();
    assert!(item.rest.contains_key("future"));
    let content: FunctionOutputContent = serde_json::from_value(serde_json::json!({
        "type":"input_image","detail":null,"image_url":"https://example.com/a.png"
    }))
    .unwrap();
    assert!(matches!(content, FunctionOutputContent::Image(_)));
}

#[test]
fn reasoning_and_extended_items_are_typed() {
    let item: ReasoningItem = serde_json::from_value(serde_json::json!({
        "type":"reasoning","id":"r","summary":[],"content":[{"type":"reasoning_text","text":"x"}],"status":"completed"
    })).unwrap();
    assert!(item.rest.is_empty());
    let extra: AdditionalTools = serde_json::from_value(serde_json::json!({
        "type":"additional_tools","id":null,"role":"developer","tools":[]
    }))
    .unwrap();
    assert!(extra.rest.is_empty());
}

#[test]
fn computer_action_and_output_have_distinct_tags() {
    let action: ComputerAction =
        serde_json::from_value(serde_json::json!({"type":"screenshot"})).unwrap();
    assert!(matches!(action, ComputerAction::Screenshot(_)));
    let output: ComputerToolCallOutput = serde_json::from_value(serde_json::json!({"type":"computer_screenshot","file_id":"f","image_url":"https://example.com/screenshot.png","future":true})).unwrap();
    assert_eq!(output.type_, ComputerScreenshotType::ComputerScreenshot);
    assert!(output.rest.contains_key("future"));
    let minimal: ReasoningItem =
        serde_json::from_value(serde_json::json!({"type":"reasoning","id":"r","summary":[]}))
            .unwrap();
    assert!(minimal.content.is_none() && minimal.status.is_none());
}

// Source: Get input token counts.md:40-4075. All 32 input branches carry
// their documented fields; the explicit variant/rest checks catch flatten
// accidentally masking incomplete modelling even when JSON round trips.
#[test]
fn every_documented_input_item_has_typed_known_fields() {
    use serde_json::json;
    macro_rules! item {
        ($variant:ident, $wire:expr) => {{
            let wire = $wire;
            let parsed: InputItem = serde_json::from_value(wire.clone()).unwrap();
            match &parsed {
                InputItem::$variant(v) => {
                    assert!(v.rest.is_empty(), "{}: {:?}", stringify!($variant), v.rest)
                }
                other => panic!("expected {}, got {other:?}", stringify!($variant)),
            }
            assert_eq!(
                serde_json::to_value(parsed).unwrap(),
                wire,
                "{}",
                stringify!($variant)
            );
        }};
    }
    item!(
        Easy,
        json!({"type":"message","role":"assistant","content":"prior answer","phase":null})
    );
    item!(
        Message,
        json!({"role":"developer","content":[{"type":"input_text","text":"x"}],"status":"completed"})
    );
    item!(
        OutputMessage,
        json!({"type":"message","id":"m","role":"assistant","status":"completed","phase":"final_answer","content":[{"type":"output_text","text":"answer","annotations":[],"logprobs":[]}]})
    );
    item!(
        FileSearchCall,
        json!({"type":"file_search_call","id":"f","queries":["x"],"status":"searching","results":[{"attributes":{"tag":"a","rank":1.5,"active":true},"file_id":"f","filename":"a.txt","score":0.5,"text":"x"}]})
    );
    item!(
        ComputerCall,
        json!({"type":"computer_call","id":"c","call_id":"c","status":"in_progress","pending_safety_checks":[{"id":"s","code":null,"message":null}],"action":{"type":"wait"},"actions":[{"type":"screenshot"}]})
    );
    item!(
        ComputerCallOutput,
        json!({"type":"computer_call_output","call_id":"c","id":null,"output":{"type":"computer_screenshot","file_id":"f","image_url":"https://x"},"acknowledged_safety_checks":null,"status":null})
    );
    item!(
        WebSearchCall,
        json!({"type":"web_search_call","id":"w","status":"failed","action":{"type":"search","query":"x","queries":["x"],"sources":[{"type":"url","url":"https://x"}]}})
    );
    item!(
        FunctionCall,
        json!({"type":"function_call","id":"f","call_id":"c","name":"f","namespace":"ns","arguments":"{}","caller":{"type":"program","caller_id":"p"},"status":"incomplete"})
    );
    item!(
        FunctionCallOutput,
        json!({"type":"function_call_output","id":null,"call_id":"c","output":[{"type":"input_image","detail":null,"image_url":null,"file_id":null,"prompt_cache_breakpoint":null}],"name":null,"namespace":null,"caller":null,"status":null})
    );
    item!(
        ToolSearchCall,
        json!({"type":"tool_search_call","arguments":{"arbitrary":[true,1]},"id":null,"call_id":null,"execution":"client","status":null})
    );
    item!(
        ToolSearchOutput,
        json!({"type":"tool_search_output","tools":[],"id":null,"call_id":null,"execution":"server","status":null})
    );
    item!(
        AdditionalTools,
        json!({"type":"additional_tools","role":"developer","tools":[],"id":null})
    );
    item!(
        Reasoning,
        json!({"type":"reasoning","id":"r","summary":[{"type":"summary_text","text":"summary"}],"content":[{"type":"reasoning_text","text":"reason"}],"encrypted_content":null,"status":"completed"})
    );
    item!(
        Compaction,
        json!({"type":"compaction","encrypted_content":"data","id":null})
    );
    item!(
        ImageGenerationCall,
        json!({"type":"image_generation_call","id":"i","result":null,"status":"generating"})
    );
    item!(
        CodeInterpreterCall,
        json!({"type":"code_interpreter_call","id":"i","container_id":"c","code":null,"outputs":null,"status":"interpreting"})
    );
    item!(
        LocalShellCall,
        json!({"type":"local_shell_call","id":"l","call_id":"c","status":"completed","action":{"type":"exec","command":["pwd"],"env":{"LANG":"C"},"timeout_ms":null,"user":null,"working_directory":null}})
    );
    item!(
        LocalShellCallOutput,
        json!({"type":"local_shell_call_output","id":"l","output":"x","status":null})
    );
    item!(
        ShellCall,
        json!({"type":"shell_call","call_id":"c","action":{"commands":["pwd"],"max_output_length":null,"timeout_ms":null},"id":null,"caller":null,"environment":null,"status":null})
    );
    item!(
        ShellCallOutput,
        json!({"type":"shell_call_output","call_id":"c","output":[{"stdout":"","stderr":"","outcome":{"type":"exit","exit_code":0}}],"id":null,"caller":null,"max_output_length":null,"status":null})
    );
    item!(
        ApplyPatchCall,
        json!({"type":"apply_patch_call","call_id":"c","operation":{"type":"delete_file","path":"a"},"status":"in_progress","id":null,"caller":null})
    );
    item!(
        ApplyPatchCallOutput,
        json!({"type":"apply_patch_call_output","call_id":"c","status":"failed","output":null,"id":null,"caller":null})
    );
    item!(
        McpListTools,
        json!({"type":"mcp_list_tools","id":"m","server_label":"s","tools":[{"name":"f","input_schema":{"type":"object"},"annotations":null,"description":null}],"error":null})
    );
    item!(
        McpApprovalRequest,
        json!({"type":"mcp_approval_request","id":"m","arguments":"{}","name":"f","server_label":"s"})
    );
    item!(
        McpApprovalResponse,
        json!({"type":"mcp_approval_response","approval_request_id":"m","approve":false,"id":null,"reason":null})
    );
    item!(
        McpCall,
        json!({"type":"mcp_call","id":"m","arguments":"{}","name":"f","server_label":"s","approval_request_id":null,"error":null,"output":null,"status":"calling"})
    );
    item!(
        CustomToolCallOutput,
        json!({"type":"custom_tool_call_output","call_id":"c","id":"o","caller":null,"output":[{"type":"input_image","detail":"original","file_id":null,"image_url":null}]})
    );
    item!(
        CustomToolCall,
        json!({"type":"custom_tool_call","call_id":"c","input":"raw","name":"f","id":"i","caller":null,"namespace":"n"})
    );
    item!(CompactionTrigger, json!({"type":"compaction_trigger"}));
    item!(ItemReference, json!({"id":"r","type":null}));
    item!(
        Program,
        json!({"type":"program","id":"p","call_id":"c","code":"1","fingerprint":"fp"})
    );
    item!(
        ProgramOutput,
        json!({"type":"program_output","id":"p","call_id":"c","result":"1","status":"incomplete"})
    );
}

#[test]
fn nested_content_fields_are_typed_with_their_distinct_contracts() {
    use serde_json::json;
    macro_rules! known {
        ($ty:ty, $wire:expr) => {{
            let wire = $wire;
            let value: $ty = serde_json::from_value(wire.clone()).unwrap();
            assert!(
                value.rest.is_empty(),
                "{}: {:?}",
                stringify!($ty),
                value.rest
            );
            assert_eq!(serde_json::to_value(&value).unwrap(), wire);
            value
        }};
    }
    let text = known!(
        ResponseInputText,
        json!({"type":"input_text","text":"x","prompt_cache_breakpoint":{"mode":"explicit"}})
    );
    assert!(text.prompt_cache_breakpoint.unwrap().rest.is_empty());
    let image = known!(
        ResponseInputImage,
        json!({"type":"input_image","detail":"original","file_id":null,"image_url":null,"prompt_cache_breakpoint":{"mode":"explicit"}})
    );
    assert!(image.prompt_cache_breakpoint.unwrap().rest.is_empty());
    known!(
        ResponseInputFile,
        json!({"type":"input_file","detail":"high","file_data":"data","file_id":null,"file_url":"https://x","filename":"file.pdf","prompt_cache_breakpoint":{"mode":"explicit"}})
    );
    known!(
        FunctionOutputText,
        json!({"type":"input_text","text":"x","prompt_cache_breakpoint":null})
    );
    known!(
        FunctionOutputImage,
        json!({"type":"input_image","detail":null,"file_id":null,"image_url":null,"prompt_cache_breakpoint":null})
    );
    known!(
        FunctionOutputFile,
        json!({"type":"input_file","detail":"low","file_data":null,"file_id":null,"file_url":null,"filename":null,"prompt_cache_breakpoint":null})
    );
    known!(
        FileSearchResult,
        json!({"attributes":{"tag":"a","rank":1.5,"active":true},"file_id":"f","filename":"a.txt","score":0.5,"text":"x"})
    );
    known!(
        PendingSafetyCheck,
        json!({"id":"s","code":null,"message":null})
    );
    known!(
        ComputerScreenshot,
        json!({"type":"computer_screenshot","file_id":"f","image_url":"https://x"})
    );
    known!(
        McpToolDefinition,
        json!({"name":"f","input_schema":["arbitrary",true],"annotations":null,"description":null})
    );
    known!(
        LocalShellAction,
        json!({"type":"exec","command":["pwd"],"env":{"LANG":"C"},"timeout_ms":null,"user":null,"working_directory":null})
    );
    known!(
        ShellAction,
        json!({"commands":["pwd"],"max_output_length":null,"timeout_ms":null})
    );
    for wire in [
        json!({"type":"direct"}),
        json!({"type":"program","caller_id":"p"}),
    ] {
        let caller: Caller = serde_json::from_value(wire.clone()).unwrap();
        match &caller {
            Caller::Direct(v) => assert!(v.rest.is_empty()),
            Caller::Program(v) => assert!(v.rest.is_empty()),
        }
        assert_eq!(serde_json::to_value(caller).unwrap(), wire);
    }
    let text = known!(
        ResponseOutputText,
        json!({"type":"output_text","text":"x","annotations":[
        {"type":"file_citation","file_id":"f","filename":"f.txt","index":0},
        {"type":"url_citation","start_index":0,"end_index":1,"title":"t","url":"https://x"},
        {"type":"container_file_citation","container_id":"c","start_index":0,"end_index":1,"file_id":"f","filename":"f.txt"},
        {"type":"file_path","file_id":"f","index":0}
    ],"logprobs":[{"token":"x","bytes":[120],"logprob":-0.5,"top_logprobs":[{"token":"x","bytes":[120],"logprob":-0.5}]}]})
    );
    for annotation in text.annotations {
        match annotation {
            OutputAnnotation::File(v) => assert!(v.rest.is_empty()),
            OutputAnnotation::Url(v) => assert!(v.rest.is_empty()),
            OutputAnnotation::ContainerFile(v) => assert!(v.rest.is_empty()),
            OutputAnnotation::Path(v) => assert!(v.rest.is_empty()),
        }
    }
    assert!(text.logprobs[0].rest.is_empty());
    assert!(text.logprobs[0].top_logprobs[0].rest.is_empty());
    known!(
        ResponseOutputRefusal,
        json!({"type":"refusal","refusal":"no"})
    );
    known!(
        ReasoningContent,
        json!({"type":"reasoning_text","text":"x"})
    );
    known!(SummaryText, json!({"type":"summary_text","text":"x"}));
    known!(
        WebSearchQuery,
        json!({"queries":["x"],"query":"x","sources":[{"type":"url","url":"https://x"}]})
    );
    known!(WebSearchSource, json!({"type":"url","url":"https://x"}));
    known!(WebSearchOpenPage, json!({"url":null}));
    known!(
        WebSearchFindInPage,
        json!({"pattern":"x","url":"https://x"})
    );
    known!(CodeLogs, json!({"logs":"x"}));
    known!(CodeImage, json!({"url":"https://x"}));
}

#[test]
fn all_action_and_outcome_variants_keep_known_fields_out_of_rest() {
    use serde_json::json;
    macro_rules! action {
        ($variant:ident,$wire:expr) => {{
            let wire = $wire;
            let action: ComputerAction = serde_json::from_value(wire.clone()).unwrap();
            match &action {
                ComputerAction::$variant(v) => assert!(v.rest.is_empty()),
                _ => panic!(),
            };
            assert_eq!(serde_json::to_value(action).unwrap(), wire);
        }};
    }
    action!(
        Click,
        json!({"type":"click","button":"back","x":1,"y":2,"keys":null})
    );
    action!(
        DoubleClick,
        json!({"type":"double_click","x":1,"y":2,"keys":null})
    );
    action!(
        Drag,
        json!({"type":"drag","path":[{"x":1,"y":2}],"keys":null})
    );
    action!(Keypress, json!({"type":"keypress","keys":["CTRL"]}));
    action!(Move, json!({"type":"move","x":1,"y":2,"keys":null}));
    action!(Screenshot, json!({"type":"screenshot"}));
    action!(
        Scroll,
        json!({"type":"scroll","scroll_x":1,"scroll_y":2,"x":3,"y":4,"keys":null})
    );
    action!(Type, json!({"type":"type","text":"x"}));
    action!(Wait, json!({"type":"wait"}));
    for wire in [
        json!({"type":"create_file","path":"a","diff":"d"}),
        json!({"type":"delete_file","path":"a"}),
        json!({"type":"update_file","path":"a","diff":"d"}),
    ] {
        let operation: ApplyPatchOperation = serde_json::from_value(wire.clone()).unwrap();
        match &operation {
            ApplyPatchOperation::Create(v) => assert!(v.rest.is_empty()),
            ApplyPatchOperation::Delete(v) => assert!(v.rest.is_empty()),
            ApplyPatchOperation::Update(v) => assert!(v.rest.is_empty()),
        }
        assert_eq!(serde_json::to_value(operation).unwrap(), wire);
    }
    for outcome in [
        json!({"type":"timeout"}),
        json!({"type":"exit","exit_code":0}),
    ] {
        let wire = json!({"stdout":"x","stderr":"","outcome":outcome});
        let output: ShellOutputContent = serde_json::from_value(wire.clone()).unwrap();
        assert!(output.rest.is_empty());
        match &output.outcome {
            ShellOutputOutcome::Timeout(v) => assert!(v.rest.is_empty()),
            ShellOutputOutcome::Exit(v) => assert!(v.rest.is_empty()),
        }
        assert_eq!(serde_json::to_value(output).unwrap(), wire);
    }
}

#[test]
fn optional_nullable_fields_preserve_absent_null_and_value() {
    use serde::{Serialize, de::DeserializeOwned};
    use serde_json::{Value, json};
    fn three<T: DeserializeOwned + Serialize>(base: Value, field: &str, value: Value) {
        for value in [None, Some(Value::Null), Some(value)] {
            let mut wire = base.clone();
            if let Some(v) = value {
                wire[field] = v;
            }
            let parsed: T = serde_json::from_value(wire.clone()).unwrap();
            assert_eq!(
                serde_json::to_value(parsed).unwrap(),
                wire,
                "{}.{field}",
                std::any::type_name::<T>()
            );
        }
    }
    three::<ResponseInputImage>(
        json!({"type":"input_image","detail":"auto"}),
        "file_id",
        json!("f"),
    );
    three::<FunctionOutputText>(
        json!({"type":"input_text","text":"x"}),
        "prompt_cache_breakpoint",
        json!({"mode":"explicit"}),
    );
    three::<FunctionOutputImage>(json!({"type":"input_image"}), "detail", json!("original"));
    for field in ["file_data", "file_id", "file_url", "filename"] {
        three::<FunctionOutputFile>(json!({"type":"input_file"}), field, json!("x"));
    }
    three::<FunctionCall>(
        json!({"type":"function_call","call_id":"c","name":"f","arguments":"{}"}),
        "caller",
        json!({"type":"direct"}),
    );
    for field in ["id", "name", "namespace"] {
        three::<FunctionCallOutput>(
            json!({"type":"function_call_output","call_id":"c","output":"x"}),
            field,
            json!("x"),
        );
    }
    three::<FunctionCallOutput>(
        json!({"type":"function_call_output","call_id":"c","output":"x"}),
        "status",
        json!("completed"),
    );
    three::<FileSearchCall>(
        json!({"type":"file_search_call","id":"f","queries":[],"status":"completed"}),
        "results",
        json!([]),
    );
    three::<FileSearchResult>(json!({}), "attributes", json!({"a":true}));
    three::<ComputerCallOutput>(
        json!({"type":"computer_call_output","call_id":"c","output":{"type":"computer_screenshot"}}),
        "acknowledged_safety_checks",
        json!([]),
    );
    three::<ToolSearchCall>(
        json!({"type":"tool_search_call","arguments":null}),
        "call_id",
        json!("c"),
    );
    three::<McpToolDefinition>(
        json!({"name":"f","input_schema":null}),
        "annotations",
        json!([true]),
    );
    three::<ApplyPatchCallOutput>(
        json!({"type":"apply_patch_call_output","call_id":"c","status":"failed"}),
        "output",
        json!("error"),
    );
    three::<ShellCall>(
        json!({"type":"shell_call","call_id":"c","action":{"commands":[]}}),
        "environment",
        json!({"type":"container_reference","container_id":"c"}),
    );
    three::<LocalShellAction>(
        json!({"type":"exec","command":[],"env":{}}),
        "timeout_ms",
        json!(100),
    );
    three::<ShellAction>(json!({"commands":[]}), "max_output_length", json!(100));
    three::<WebSearchOpenPage>(json!({}), "url", json!("https://x"));
    three::<ToolChoiceMcp>(json!({"type":"mcp","server_label":"s"}), "name", json!("f"));
}

#[test]
fn required_nullable_and_nonnullable_fields_have_distinct_contracts() {
    use serde::de::DeserializeOwned;
    use serde_json::{Value, json};
    fn required<T: DeserializeOwned>(wire: Value, fields: &[&str]) {
        assert!(serde_json::from_value::<T>(wire.clone()).is_ok());
        for field in fields {
            let mut v = wire.clone();
            v.as_object_mut().unwrap().remove(*field);
            assert!(
                serde_json::from_value::<T>(v).is_err(),
                "{} missing {field}",
                std::any::type_name::<T>()
            );
        }
    }
    required::<ImageGenerationCall>(
        json!({"type":"image_generation_call","id":"i","result":null,"status":"failed"}),
        &["type", "id", "result", "status"],
    );
    required::<CodeInterpreterCall>(
        json!({"type":"code_interpreter_call","id":"i","code":null,"container_id":"c","outputs":null,"status":"failed"}),
        &["code", "outputs", "status"],
    );
    required::<DoubleClickAction>(json!({"x":1,"y":2,"keys":null}), &["keys"]);
    required::<ResponseOutputText>(
        json!({"type":"output_text","text":"x","annotations":[],"logprobs":[]}),
        &["annotations", "logprobs"],
    );
    required::<ResponseOutputMessage>(
        json!({"type":"message","id":"i","content":[],"role":"assistant","status":"completed"}),
        &["type", "id", "content", "role", "status"],
    );
    required::<LocalShellAction>(
        json!({"command":[],"env":{},"type":"exec"}),
        &["env", "type"],
    );
    required::<ApplyPatchCall>(
        json!({"type":"apply_patch_call","call_id":"c","operation":{"type":"delete_file","path":"p"},"status":"completed"}),
        &["status"],
    );
    required::<Program>(
        json!({"type":"program","id":"p","call_id":"c","code":"x","fingerprint":"f"}),
        &["fingerprint"],
    );
    required::<ProgramOutput>(
        json!({"type":"program_output","id":"p","call_id":"c","result":"x","status":"completed"}),
        &["status"],
    );
    required::<McpToolDefinition>(json!({"name":"f","input_schema":null}), &["input_schema"]);
    required::<ToolSearchCall>(
        json!({"type":"tool_search_call","arguments":null}),
        &["arguments"],
    );
    // D:797-801 permits absence but not null for screenshot file_id/image_url.
    assert!(
        serde_json::from_value::<ComputerScreenshot>(
            json!({"type":"computer_screenshot","image_url":null})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<ResponseInputText>(
            json!({"type":"input_text","text":"x","prompt_cache_breakpoint":null})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<FunctionOutputFile>(json!({"type":"input_file","detail":null}))
            .is_err()
    );
    assert!(
        serde_json::from_value::<LocalShellAction>(
            json!({"type":"exec","command":[],"env":{"k":1}})
        )
        .is_err()
    );
    assert!(serde_json::from_value::<FileSearchResult>(json!({"attributes":{"k":[]}})).is_err());
    assert!(
        serde_json::from_value::<ShellCallEnvironment>(json!({"type":"container_auto"})).is_err()
    );
    assert!(serde_json::from_value::<CustomOutput>(json!([{"type":"input_image"}])).is_err());
    assert!(serde_json::from_value::<FunctionOutput>(json!([{"type":"input_image"}])).is_ok());
}

#[test]
fn status_and_configuration_enums_match_the_documented_closed_sets() {
    use serde::{Serialize, de::DeserializeOwned};
    use serde_json::json;
    fn closed<T: DeserializeOwned + Serialize>(values: &[&str]) {
        for value in values {
            let v: T = serde_json::from_value(json!(value)).unwrap();
            assert_eq!(serde_json::to_value(v).unwrap(), json!(value));
        }
        assert!(serde_json::from_value::<T>(json!("undocumented")).is_err());
    }
    closed::<ItemStatus>(&["in_progress", "completed", "incomplete"]);
    closed::<WebSearchStatus>(&["in_progress", "searching", "completed", "failed"]);
    closed::<FileSearchStatus>(&[
        "in_progress",
        "searching",
        "completed",
        "incomplete",
        "failed",
    ]);
    closed::<ImageGenerationStatus>(&["in_progress", "completed", "generating", "failed"]);
    closed::<CodeInterpreterStatus>(&[
        "in_progress",
        "completed",
        "incomplete",
        "interpreting",
        "failed",
    ]);
    closed::<McpCallStatus>(&[
        "in_progress",
        "completed",
        "incomplete",
        "calling",
        "failed",
    ]);
    closed::<ApplyPatchStatus>(&["in_progress", "completed"]);
    closed::<ApplyPatchOutputStatus>(&["completed", "failed"]);
    closed::<ProgramOutputStatus>(&["completed", "incomplete"]);
    closed::<ImageDetail>(&["low", "high", "auto", "original"]);
    closed::<FileDetail>(&["auto", "low", "high"]);
    closed::<PromptCacheMode>(&["explicit"]);
    closed::<ReasoningEffort>(&["none", "minimal", "low", "medium", "high", "xhigh", "max"]);
    closed::<ReasoningContext>(&["auto", "current_turn", "all_turns"]);
    closed::<ReasoningSummary>(&["auto", "concise", "detailed"]);
    closed::<TextVerbosity>(&["low", "medium", "high"]);
    assert!(serde_json::from_value::<ApplyPatchStatus>(json!("incomplete")).is_err());
    assert!(serde_json::from_value::<ApplyPatchOutputStatus>(json!("in_progress")).is_err());
    assert!(serde_json::from_value::<ItemStatus>(json!("failed")).is_err());
    assert!(serde_json::from_value::<McpCall>(json!({"type":"mcp_call","id":"m","arguments":"{}","name":"f","server_label":"s","status":null})).is_err());
}

#[test]
fn count_configuration_and_tool_choice_contracts_preserve_null_and_tags() {
    use gproxy_protocol::openai::count_tokens::CountTokensRequestBody;
    use serde_json::json;
    let wire = json!({"conversation":null,"input":null,"instructions":null,"model":null,"parallel_tool_calls":null,"previous_response_id":null,"reasoning":null,"text":null,"tool_choice":null,"tools":null});
    let body: CountTokensRequestBody = serde_json::from_value(wire.clone()).unwrap();
    assert!(body.rest.is_empty());
    assert_eq!(serde_json::to_value(body).unwrap(), wire);
    for field in ["personality", "truncation"] {
        let mut wire = json!({});
        wire[field] = json!(null);
        assert!(serde_json::from_value::<CountTokensRequestBody>(wire).is_err());
    }
    for wire in [
        json!({"effort":null,"generate_summary":null,"summary":null,"context":null,"mode":"future-mode"}),
        json!({"effort":"max","generate_summary":"concise","summary":"detailed","context":"all_turns"}),
    ] {
        let config: ReasoningConfig = serde_json::from_value(wire.clone()).unwrap();
        assert!(config.rest.is_empty());
        assert_eq!(serde_json::to_value(config).unwrap(), wire);
    }
    for format in [
        json!({"type":"text"}),
        json!({"type":"json_object"}),
        json!({"type":"json_schema","name":"n","schema":{"arbitrary":[true]},"description":"d","strict":null}),
    ] {
        let wire = json!({"format":format,"verbosity":null});
        let config: TextConfig = serde_json::from_value(wire.clone()).unwrap();
        assert!(config.rest.is_empty());
        match config.format.as_ref().unwrap() {
            TextFormat::Text(v) => assert!(v.rest.is_empty()),
            TextFormat::JsonObject(v) => assert!(v.rest.is_empty()),
            TextFormat::JsonSchema(v) => assert!(v.rest.is_empty()),
            #[cfg(not(feature = "exhaustive"))]
            _ => panic!(),
        }
        assert_eq!(serde_json::to_value(config).unwrap(), wire);
    }
    for wire in [
        json!("auto"),
        json!({"type":"allowed_tools","mode":"required","tools":[{"arbitrary":true}]}),
        json!({"type":"file_search"}),
        json!({"type":"function","name":"f"}),
        json!({"type":"mcp","server_label":"s","name":null}),
        json!({"type":"custom","name":"f"}),
        json!({"type":"programmatic_tool_calling"}),
        json!({"type":"apply_patch"}),
        json!({"type":"shell"}),
    ] {
        let choice: ToolChoice = serde_json::from_value(wire.clone()).unwrap();
        match &choice {
            ToolChoice::Mode(_) => {}
            ToolChoice::Allowed(v) => assert!(v.rest.is_empty()),
            ToolChoice::Hosted(v) => assert!(v.rest.is_empty()),
            ToolChoice::Function(v) => assert!(v.rest.is_empty()),
            ToolChoice::Mcp(v) => assert!(v.rest.is_empty()),
            ToolChoice::Custom(v) => assert!(v.rest.is_empty()),
            ToolChoice::Programmatic(v) => assert!(v.rest.is_empty()),
            ToolChoice::ApplyPatch(v) => assert!(v.rest.is_empty()),
            ToolChoice::Shell(v) => assert!(v.rest.is_empty()),
            #[cfg(not(feature = "exhaustive"))]
            _ => panic!(),
        }
        assert_eq!(serde_json::to_value(choice).unwrap(), wire);
    }
}

#[test]
fn typed_builders_emit_required_fields_and_nullable_values() {
    use serde_json::json;
    let image = ImageGenerationCall::builder(
        ImageGenerationCallType::ImageGenerationCall,
        "i".into(),
        None,
        ImageGenerationStatus::Failed,
    )
    .build();
    assert_eq!(
        serde_json::to_value(image).unwrap(),
        json!({"type":"image_generation_call","id":"i","result":null,"status":"failed"})
    );
    let message = ResponseOutputMessage::builder(
        "m".into(),
        vec![],
        OutputMessageRole::Assistant,
        OutputMessageStatus::Completed,
        MessageType::Message,
    )
    .build();
    assert_eq!(serde_json::to_value(message).unwrap()["type"], "message");
    let program = Program::builder(
        ProgramType::Program,
        "p".into(),
        "c".into(),
        "code".into(),
        "fp".into(),
    )
    .build();
    assert_eq!(serde_json::to_value(program).unwrap()["fingerprint"], "fp");
    let output = ApplyPatchCallOutput::builder(
        ApplyPatchCallOutputType::ApplyPatchCallOutput,
        "c".into(),
        ApplyPatchOutputStatus::Failed,
    )
    .output(None)
    .build();
    assert_eq!(
        serde_json::to_value(output).unwrap(),
        json!({"type":"apply_patch_call_output","call_id":"c","status":"failed","output":null})
    );
    let file = FunctionOutputFile::builder(ResponseInputFileType::ResponseInputFile)
        .filename(None)
        .detail(FileDetail::High)
        .build();
    assert_eq!(
        serde_json::to_value(file).unwrap(),
        json!({"type":"input_file","filename":null,"detail":"high"})
    );
    let path: OutputAnnotation = serde_json::from_value(
        json!({"type":"file_path","file_id":"f","index":0,"filename":"extension"}),
    )
    .unwrap();
    match path {
        OutputAnnotation::Path(v) => assert_eq!(v.rest["filename"], "extension"),
        _ => panic!(),
    }
}
