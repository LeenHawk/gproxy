//! Responses request-body shaping for the ChatGPT Codex backend.

use bytes::Bytes;
use serde_json::{Value, json};

use crate::channel::ShapeCtx;
use crate::channel::settings::RequestShapeSettings;
use crate::channel::shaping::{self, openai_cache};

const APPLY_PATCH_GRAMMAR: &str = include_str!("apply_patch.lark");

const STRIP_KEYS: &[&str] = &[
    "max_output_tokens",
    "metadata",
    "prompt_cache_options",
    "temperature",
    "top_p",
    "top_logprobs",
    "safety_identifier",
    "truncation",
];

pub(super) fn shape(body: Bytes, ctx: &ShapeCtx) -> Bytes {
    let settings = RequestShapeSettings::from_value(ctx.settings);
    let body = match settings
        .enable_openai_magic_cache
        .then(|| openai_cache::kind_for_operation(ctx.op))
        .flatten()
    {
        Some(kind) => shaping::with_json_body(body, |value| {
            openai_cache::apply_magic_string_cache_breakpoints(value, kind)
        }),
        None => body,
    };
    normalize_responses_body(&body)
}

/// Force streaming/non-persistence, drop unsupported sampling fields, and lift
/// system input messages into top-level instructions. Invalid JSON is unchanged.
fn normalize_responses_body(body: &Bytes) -> Bytes {
    let Ok(mut value) = serde_json::from_slice::<Value>(body) else {
        return body.clone();
    };
    let Some(object) = value.as_object_mut() else {
        return body.clone();
    };

    object.insert("stream".into(), Value::Bool(true));
    object.insert("store".into(), Value::Bool(false));
    for key in STRIP_KEYS {
        object.remove(*key);
    }

    let mut instructions = object
        .get("instructions")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if let Some(text) = object.get("input").and_then(Value::as_str) {
        let text = text.to_string();
        object.insert(
            "input".into(),
            json!([{ "type": "message", "role": "user", "content": text }]),
        );
    }
    if let Some(Value::Array(items)) = object.get_mut("input") {
        let mut retained = Vec::with_capacity(items.len());
        for item in std::mem::take(items) {
            if is_system_role(&item) {
                append_instruction(&mut instructions, item_text(&item));
            } else {
                retained.push(item);
            }
        }
        *items = retained;
    }

    normalize_codex_tools(object);
    normalize_codex_tool_history(object);

    object.insert("instructions".into(), Value::String(instructions));
    super::input_shape::strip_reasoning_status(object);
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .unwrap_or_else(|_| body.clone())
}

fn normalize_codex_tools(object: &mut serde_json::Map<String, Value>) {
    let Some(Value::Array(tools)) = object.get_mut("tools") else {
        return;
    };
    let mut shell = false;
    let mut apply_patch = false;
    for tool in tools {
        match tool.get("type").and_then(Value::as_str) {
            Some("shell") | Some("local_shell") => {
                shell = true;
                *tool = shell_command_tool();
            }
            Some("apply_patch") => {
                apply_patch = true;
                *tool = apply_patch_tool();
            }
            Some("tool_search") => normalize_tool_search(tool),
            _ => {}
        }
    }
    normalize_codex_tool_choice(object, shell, apply_patch);
}

fn shell_command_tool() -> Value {
    json!({
        "type": "function",
        "name": "shell_command",
        "description": "Runs a shell command and returns its output.",
        "strict": false,
        "parameters": {
            "type": "object",
            "properties": {
                "command": {"type": "string", "description": "Shell script to run."},
                "workdir": {"type": "string", "description": "Working directory for the command."},
                "timeout_ms": {"type": "number", "description": "Maximum command runtime."}
            },
            "required": ["command"],
            "additionalProperties": false
        }
    })
}

fn apply_patch_tool() -> Value {
    json!({
        "type": "custom",
        "name": "apply_patch",
        "description": "The apply_patch tool can be used to edit files. This is a FREEFORM tool, so do not wrap the patch in JSON.",
        "format": {
            "type": "grammar",
            "syntax": "lark",
            "definition": APPLY_PATCH_GRAMMAR
        }
    })
}

fn normalize_tool_search(tool: &mut Value) {
    let Some(object) = tool.as_object_mut() else {
        return;
    };
    match object.get("execution").and_then(Value::as_str) {
        Some("client") => {
            object.entry("description").or_insert_with(|| {
                Value::String("Search deferred tools using a regular expression".to_owned())
            });
            object.entry("parameters").or_insert_with(|| {
                json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string", "description": "Search query for deferred tools."},
                        "limit": {"type": "number", "description": "Maximum tools to return."}
                    },
                    "required": ["query"],
                    "additionalProperties": false
                })
            });
        }
        _ => {
            object.remove("description");
            object.remove("parameters");
        }
    }
}

fn normalize_codex_tool_choice(
    object: &mut serde_json::Map<String, Value>,
    shell: bool,
    apply_patch: bool,
) {
    let Some(choice) = object.get_mut("tool_choice").and_then(Value::as_object_mut) else {
        return;
    };
    let name = choice.get("name").and_then(Value::as_str);
    if shell && matches!(name, Some("bash" | "shell" | "shell_command")) {
        choice.insert("type".to_owned(), Value::String("function".to_owned()));
        choice.insert("name".to_owned(), Value::String("shell_command".to_owned()));
    } else if apply_patch
        && matches!(
            name,
            Some("str_replace_editor" | "str_replace_based_edit_tool" | "apply_patch")
        )
    {
        choice.insert("type".to_owned(), Value::String("custom".to_owned()));
        choice.insert("name".to_owned(), Value::String("apply_patch".to_owned()));
    }
}

fn normalize_codex_tool_history(object: &mut serde_json::Map<String, Value>) {
    let Some(Value::Array(items)) = object.get_mut("input") else {
        return;
    };
    for item in items {
        let Some(kind) = item.get("type").and_then(Value::as_str) else {
            continue;
        };
        *item = match kind {
            "shell_call" | "local_shell_call" => shell_call_to_function(item),
            "shell_call_output" | "local_shell_call_output" => {
                tool_output_to_codex(item, "function_call_output")
            }
            "apply_patch_call" => apply_patch_call_to_custom(item),
            "apply_patch_call_output" => tool_output_to_codex(item, "custom_tool_call_output"),
            _ => continue,
        };
    }
}

fn shell_call_to_function(item: &Value) -> Value {
    let action = item.get("action").and_then(Value::as_object);
    let command = action
        .and_then(|action| action.get("commands"))
        .and_then(Value::as_array)
        .map(|commands| {
            commands
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .or_else(|| {
            action
                .and_then(|action| action.get("command"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .unwrap_or_default();
    let mut arguments = json!({"command": command});
    if let Some(timeout) = action.and_then(|action| action.get("timeout_ms")).cloned() {
        arguments["timeout_ms"] = timeout;
    }
    let output = json!({
        "type": "function_call",
        "id": mapped_item_id(item, "fc_"),
        "call_id": item.get("call_id").cloned().unwrap_or(Value::Null),
        "name": "shell_command",
        "arguments": serde_json::to_string(&arguments).unwrap_or_else(|_| "{}".to_owned()),
        "status": item.get("status").cloned().unwrap_or_else(|| Value::String("completed".to_owned()))
    });
    output
}

fn apply_patch_call_to_custom(item: &Value) -> Value {
    let output = json!({
        "type": "custom_tool_call",
        "id": mapped_item_id(item, "ctc_"),
        "call_id": item.get("call_id").cloned().unwrap_or(Value::Null),
        "name": "apply_patch",
        "input": patch_text(item.get("operation").unwrap_or(&Value::Null))
    });
    output
}

fn mapped_item_id(item: &Value, prefix: &str) -> Value {
    let source = item
        .get("id")
        .and_then(Value::as_str)
        .or_else(|| item.get("call_id").and_then(Value::as_str));
    let Some(source) = source else {
        return Value::Null;
    };
    if source.starts_with(prefix) {
        return Value::String(source.to_owned());
    }

    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in source.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    Value::String(format!("{prefix}{hash:016x}"))
}

fn patch_text(operation: &Value) -> String {
    let kind = operation
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let path = operation
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut patch = String::from("*** Begin Patch\n");
    match kind {
        "create_file" => {
            patch.push_str("*** Add File: ");
            patch.push_str(path);
            patch.push('\n');
            let diff = operation
                .get("diff")
                .and_then(Value::as_str)
                .unwrap_or_default();
            for line in diff.split_inclusive('\n') {
                patch.push('+');
                patch.push_str(line);
            }
            if !diff.is_empty() && !diff.ends_with('\n') {
                patch.push('\n');
            }
        }
        "delete_file" => {
            patch.push_str("*** Delete File: ");
            patch.push_str(path);
            patch.push('\n');
        }
        _ => {
            patch.push_str("*** Update File: ");
            patch.push_str(path);
            patch.push('\n');
            let diff = operation
                .get("diff")
                .and_then(Value::as_str)
                .unwrap_or_default();
            patch.push_str(diff);
            if !diff.is_empty() && !diff.ends_with('\n') {
                patch.push('\n');
            }
        }
    }
    patch.push_str("*** End Patch\n");
    patch
}

fn tool_output_to_codex(item: &Value, kind: &str) -> Value {
    let output = item.get("output").map(output_text).unwrap_or_default();
    let mut value = json!({
        "type": kind,
        "call_id": item.get("call_id").cloned().unwrap_or(Value::Null),
        "output": output,
        "status": item.get("status").cloned().unwrap_or_else(|| Value::String("completed".to_owned()))
    });
    copy_optional(item, &mut value, "id");
    value
}

fn copy_optional(source: &Value, target: &mut Value, key: &str) {
    if let Some(value) = source.get(key).filter(|value| !value.is_null()).cloned() {
        target[key] = value;
    }
}

fn output_text(output: &Value) -> String {
    match output {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .flat_map(|part| [part.get("stdout"), part.get("stderr")])
            .flatten()
            .filter_map(Value::as_str)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Null => String::new(),
        value => value.to_string(),
    }
}

fn is_system_role(item: &Value) -> bool {
    item.get("role")
        .and_then(Value::as_str)
        .is_some_and(|role| role.eq_ignore_ascii_case("system"))
}

fn append_instruction(instructions: &mut String, text: String) {
    if text.is_empty() {
        return;
    }
    if !instructions.is_empty() {
        instructions.push('\n');
    }
    instructions.push_str(&text);
}

fn item_text(item: &Value) -> String {
    let mut parts = Vec::new();
    collect_text(item.get("content").unwrap_or(&Value::Null), &mut parts);
    parts.join("\n")
}

fn collect_text(value: &Value, parts: &mut Vec<String>) {
    match value {
        Value::String(text) if !text.is_empty() => parts.push(text.clone()),
        Value::Array(items) => items.iter().for_each(|item| collect_text(item, parts)),
        Value::Object(object) => {
            if let Some(text) = object.get("text").and_then(Value::as_str)
                && !text.is_empty()
            {
                parts.push(text.to_string());
            }
        }
        _ => {}
    }
}
