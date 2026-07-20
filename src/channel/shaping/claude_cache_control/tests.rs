use serde_json::json;

use super::*;

#[test]
fn manual_message_normalizes_and_indexes_flat_cacheable_blocks() {
    let mut body = json!({
        "messages": [
            {"role": "user", "content": "first"},
            {"role": "assistant", "content": [
                {"type": "thinking", "thinking": "secret", "signature": "sig"},
                {"type": "text", "text": "second"}
            ]},
            {"role": "user", "content": [{"type": "text", "text": "   "}]}
        ]
    });

    apply_manual_cache_breakpoint(&mut body, "message", Some(-1), Some("5m")).unwrap();

    assert_eq!(body["messages"][0]["content"][0]["type"], "text");
    assert!(
        body["messages"][1]["content"][0]
            .get("cache_control")
            .is_none()
    );
    assert_eq!(
        body["messages"][1]["content"][1]["cache_control"]["ttl"],
        "5m"
    );
}

#[test]
fn manual_cache_breakpoint_preserves_existing_and_enforces_four_slots() {
    let mut body = json!({
        "cache_control": {"type": "ephemeral"},
        "system": [{"type": "text", "text": "sys", "cache_control": {"type": "ephemeral"}}],
        "tools": [{"name": "a", "cache_control": {"type": "ephemeral"}}],
        "messages": [{"role": "user", "content": [
            {"type": "text", "text": "kept", "cache_control": {"type": "ephemeral", "ttl": "1h"}},
            {"type": "text", "text": "new"}
        ]}]
    });

    apply_manual_cache_breakpoint(&mut body, "message", Some(1), Some("5m")).unwrap();
    assert_eq!(
        body["messages"][0]["content"][0]["cache_control"]["ttl"],
        "1h"
    );
    assert_eq!(
        apply_manual_cache_breakpoint(&mut body, "message", Some(2), None),
        Err("Claude cache breakpoint limit reached")
    );
    assert!(
        body["messages"][0]["content"][1]
            .get("cache_control")
            .is_none()
    );
}

#[test]
fn drops_empty_user_text_block_and_message() {
    let mut body = json!({
        "messages": [
            {"role": "user", "content": ""},
            {"role": "user", "content": "hi"}
        ]
    });
    sanitize_claude_body(&mut body);
    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["content"][0]["text"], "hi");
}

#[test]
fn drops_whitespace_only_text_block() {
    let mut body = json!({
        "system": [
            {"type": "text", "text": "   \n"},
            {"type": "text", "text": "real"}
        ]
    });
    sanitize_claude_body(&mut body);
    let system = body["system"].as_array().unwrap();
    assert_eq!(system.len(), 1);
    assert_eq!(system[0]["text"], "real");
}

#[test]
fn shifts_cache_control_to_prev_block_in_same_array() {
    let mut body = json!({
        "system": [
            {"type": "text", "text": "anchor"},
            {"type": "text", "text": "  ", "cache_control": {"type": "ephemeral", "ttl": "5m"}}
        ]
    });
    sanitize_claude_body(&mut body);
    let system = body["system"].as_array().unwrap();
    assert_eq!(system.len(), 1);
    assert_eq!(system[0]["text"], "anchor");
    assert_eq!(system[0]["cache_control"]["ttl"], "5m");
}

#[test]
fn shifts_cache_control_across_messages() {
    let mut body = json!({
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "first"}]},
            {"role": "assistant", "content": [
                {"type": "text", "text": " ", "cache_control": {"type": "ephemeral"}}
            ]}
        ]
    });
    sanitize_claude_body(&mut body);
    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 1);
    let block = &messages[0]["content"][0];
    assert_eq!(block["text"], "first");
    assert_eq!(block["cache_control"]["type"], "ephemeral");
}

#[test]
fn drops_cc_when_no_prior_cacheable_block_exists() {
    let mut body = json!({
        "messages": [{"role": "user", "content": [
            {"type": "text", "text": "", "cache_control": {"type": "ephemeral"}}
        ]}]
    });
    sanitize_claude_body(&mut body);
    assert!(body["messages"].as_array().unwrap().is_empty());
}

#[test]
fn removes_system_field_when_all_blocks_drop() {
    let mut body = json!({
        "system": [{"type": "text", "text": "  "}],
        "messages": [{"role": "user", "content": "hi"}]
    });
    sanitize_claude_body(&mut body);
    assert!(body.get("system").is_none());
}

#[test]
fn preserves_non_text_blocks() {
    let mut body = json!({
        "messages": [{"role": "user", "content": [
            {"type": "image", "source": {"type": "base64", "data": "x"}},
            {"type": "text", "text": "  ", "cache_control": {"type": "ephemeral"}}
        ]}]
    });
    sanitize_claude_body(&mut body);
    let blocks = body["messages"][0]["content"].as_array().unwrap();
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0]["type"], "image");
    assert_eq!(blocks[0]["cache_control"]["type"], "ephemeral");
}

#[test]
fn does_not_shift_cache_control_to_assistant_image() {
    let mut body = json!({
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "anchor"}]},
            {"role": "assistant", "content": [
                {"type": "image", "source": {"type": "base64", "data": "x"}},
                {"type": "text", "text": "  ", "cache_control": {"type": "ephemeral"}}
            ]}
        ]
    });

    sanitize_claude_body(&mut body);

    assert!(
        body["messages"][1]["content"][0]
            .get("cache_control")
            .is_none()
    );
    assert_eq!(
        body["messages"][0]["content"][0]["cache_control"]["type"],
        "ephemeral"
    );
}

#[test]
fn trims_text_when_kept() {
    let mut body = json!({
        "messages": [{"role": "user", "content": "  hi  "}]
    });
    sanitize_claude_body(&mut body);
    assert_eq!(body["messages"][0]["content"][0]["text"], "hi");
}
