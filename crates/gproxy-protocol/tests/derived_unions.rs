use gproxy_protocol::{claude, openai::responses::*};
use serde_json::{Value, json};

#[test]
fn input_messages_keep_their_variant_when_extension_fields_are_present() {
    for content in [
        json!("hello"),
        json!([{"type": "input_text", "text": "hello"}]),
    ] {
        for explicit_type in [false, true] {
            let mut wire = json!({"role": "assistant", "content": content, "id": "extension"});
            if explicit_type {
                wire["type"] = json!("message");
            }
            let item: InputItem = serde_json::from_value(wire.clone()).unwrap();
            assert!(matches!(item, InputItem::Easy(_)));
            assert_eq!(serde_json::to_value(item).unwrap(), wire);
        }
    }

    for wire in [
        json!({"id": "reference"}),
        json!({"id": "reference", "type": null}),
    ] {
        let item: InputItem = serde_json::from_value(wire.clone()).unwrap();
        assert!(matches!(item, InputItem::ItemReference(_)));
        assert_eq!(serde_json::to_value(item).unwrap(), wire);
    }
    let wire = json!({
        "type": "function_call", "call_id": "call", "name": "lookup", "arguments": "{}",
        "role": "assistant", "content": []
    });
    let item: InputItem = serde_json::from_value(wire.clone()).unwrap();
    assert!(matches!(item, InputItem::FunctionCall(_)));
    assert_eq!(serde_json::to_value(item).unwrap(), wire);
}

#[test]
fn tool_choice_marker_shapes_have_disjoint_tags() {
    for (wire, expected) in [
        (
            json!({"type": "programmatic_tool_calling", "name": "extension"}),
            "programmatic",
        ),
        (
            json!({"type": "apply_patch", "name": "extension"}),
            "apply_patch",
        ),
        (json!({"type": "shell", "name": "extension"}), "shell"),
        (json!({"type": "function", "name": "lookup"}), "function"),
        (json!({"type": "custom", "name": "lookup"}), "custom"),
        (
            json!({"type": "mcp", "server_label": "server", "name": "lookup"}),
            "mcp",
        ),
        (
            json!({"type": "web_search_preview_2025_03_11", "name": "extension"}),
            "hosted",
        ),
    ] {
        let choice: ToolChoice = serde_json::from_value(wire.clone()).unwrap();
        let actual = match &choice {
            ToolChoice::Programmatic(_) => "programmatic",
            ToolChoice::ApplyPatch(_) => "apply_patch",
            ToolChoice::Shell(_) => "shell",
            ToolChoice::Function(_) => "function",
            ToolChoice::Custom(_) => "custom",
            ToolChoice::Mcp(_) => "mcp",
            ToolChoice::Hosted(_) => "hosted",
            _ => "unexpected",
        };
        assert_eq!(actual, expected);
        assert_eq!(serde_json::to_value(choice).unwrap(), wire);
    }
    assert!(serde_json::from_value::<ToolChoice>(json!({"type": "unknown"})).is_err());
}

#[test]
fn claude_tagged_content_ignores_other_variants_extension_fields() {
    use claude::content::{ContentBlock, ToolResultContentBlock};
    let wire = json!({"type": "tool_reference", "tool_name": "lookup", "text": "extension"});
    let block: ToolResultContentBlock = serde_json::from_value(wire.clone()).unwrap();
    assert!(matches!(block, ToolResultContentBlock::ToolReference(_)));
    assert_eq!(serde_json::to_value(block).unwrap(), wire);

    let wire = json!({
        "type": "container_upload", "file_id": "file", "text": "extension",
        "source": {"type": "url", "url": "https://example.com"}
    });
    let block: ContentBlock = serde_json::from_value(wire.clone()).unwrap();
    assert!(matches!(block, ContentBlock::ContainerUpload(_)));
    assert_eq!(serde_json::to_value(block).unwrap(), wire);
}

#[test]
fn claude_tool_versions_and_short_tags_select_the_declared_variant() {
    use claude::tools::ToolUnion;
    macro_rules! check {
        ($variant:ident, $wire:expr) => {{
            let mut wire: Value = $wire;
            wire["input_schema"] = json!({"type": "object"});
            wire["mcp_server_name"] = json!("extension");
            let tool: ToolUnion = serde_json::from_value(wire.clone()).unwrap();
            assert!(matches!(&tool, ToolUnion::$variant(_)));
            assert_eq!(serde_json::to_value(tool).unwrap(), wire);
        }};
    }
    check!(
        Bash20241022,
        json!({"name": "bash", "type": "bash_20241022"})
    );
    check!(
        Bash20250124,
        json!({"name": "bash", "type": "bash_20250124"})
    );
    check!(
        CodeExecution20250522,
        json!({"name": "code_execution", "type": "code_execution_20250522"})
    );
    check!(
        CodeExecution20250825,
        json!({"name": "code_execution", "type": "code_execution_20250825"})
    );
    check!(
        CodeExecution20260120,
        json!({"name": "code_execution", "type": "code_execution_20260120"})
    );
    check!(
        CodeExecution20260521,
        json!({"name": "code_execution", "type": "code_execution_20260521"})
    );
    check!(
        Computer20241022,
        json!({"name": "computer", "type": "computer_20241022", "display_width_px": 10, "display_height_px": 10})
    );
    check!(
        Computer20250124,
        json!({"name": "computer", "type": "computer_20250124", "display_width_px": 10, "display_height_px": 10})
    );
    check!(
        Computer20251124,
        json!({"name": "computer", "type": "computer_20251124", "display_width_px": 10, "display_height_px": 10})
    );
    check!(
        Memory20250818,
        json!({"name": "memory", "type": "memory_20250818"})
    );
    check!(
        TextEditor20241022,
        json!({"name": "str_replace_editor", "type": "text_editor_20241022"})
    );
    check!(
        TextEditor20250124,
        json!({"name": "str_replace_editor", "type": "text_editor_20250124"})
    );
    check!(
        TextEditor20250429,
        json!({"name": "str_replace_based_edit_tool", "type": "text_editor_20250429"})
    );
    check!(
        TextEditor20250728,
        json!({"name": "str_replace_based_edit_tool", "type": "text_editor_20250728"})
    );
    check!(
        WebSearch20250305,
        json!({"name": "web_search", "type": "web_search_20250305"})
    );
    check!(
        WebSearch20260209,
        json!({"name": "web_search", "type": "web_search_20260209"})
    );
    check!(
        WebSearch20260318,
        json!({"name": "web_search", "type": "web_search_20260318"})
    );
    check!(
        WebFetch20250910,
        json!({"name": "web_fetch", "type": "web_fetch_20250910"})
    );
    check!(
        WebFetch20260209,
        json!({"name": "web_fetch", "type": "web_fetch_20260209"})
    );
    check!(
        WebFetch20260309,
        json!({"name": "web_fetch", "type": "web_fetch_20260309"})
    );
    check!(
        WebFetch20260318,
        json!({"name": "web_fetch", "type": "web_fetch_20260318"})
    );
    check!(
        Advisor20260301,
        json!({"name": "advisor", "type": "advisor_20260301", "model": "claude-opus-4-6"})
    );
    for tag in ["tool_search_tool_bm25_20251119", "tool_search_tool_bm25"] {
        check!(
            ToolSearchBm2520251119,
            json!({"name": "tool_search_tool_bm25", "type": tag})
        );
    }
    for tag in ["tool_search_tool_regex_20251119", "tool_search_tool_regex"] {
        check!(
            ToolSearchRegex20251119,
            json!({"name": "tool_search_tool_regex", "type": tag})
        );
    }
    for wire in [
        json!({"name": "lookup", "input_schema": {"type": "object"}}),
        json!({"name": "lookup", "type": "custom", "input_schema": {"type": "object"}}),
    ] {
        let tool: ToolUnion = serde_json::from_value(wire.clone()).unwrap();
        assert!(matches!(&tool, ToolUnion::Custom(_)));
        assert_eq!(serde_json::to_value(tool).unwrap(), wire);
    }
    assert!(
        serde_json::from_value::<ToolUnion>(json!({"name": "bash", "type": "unknown"})).is_err()
    );
}
