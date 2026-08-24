use gproxy_channel_api::ChannelError;
use gproxy_protocol::openai::common::{
    CustomToolGrammarFormat, CustomToolGrammarFormatType, CustomToolGrammarSyntax,
    CustomToolInputFormat, FunctionToolChoiceType, ResponseCustomToolChoice,
    ResponseFunctionToolChoice, ResponseItemLifecycleStatus, ResponseToolChoice,
    ToolSearchExecution,
};
use gproxy_protocol::openai::generate_content::responses::{
    ApplyPatchOperation, ResponseFunctionParameters, ResponseFunctionStrict, ResponseItem,
    ResponseOutput, ResponseTool, ShellAction, TypedResponseItem,
};
use serde_json::{Value, json};

const APPLY_PATCH_GRAMMAR: &str = r#"start: begin_patch hunk+ end_patch
begin_patch: "*** Begin Patch" LF
end_patch: "*** End Patch" LF?
hunk: add_hunk | delete_hunk | update_hunk
add_hunk: "*** Add File: " filename LF add_line+
delete_hunk: "*** Delete File: " filename LF
update_hunk: "*** Update File: " filename LF change_move? change?
filename: /(.+)/
add_line: "+" /(.*)/ LF -> line
change_move: "*** Move to: " filename LF
change: (change_context | change_line)+ eof_line?
change_context: ("@@" | "@@ " /(.+)/) LF
change_line: ("+" | "-" | " ") /(.*)/ LF
eof_line: "*** End of File" LF
%import common.LF
"#;

pub(super) fn normalize_definitions(
    tools: &mut Option<Vec<ResponseTool>>,
    choice: &mut Option<ResponseToolChoice>,
) {
    let Some(tools) = tools else {
        return;
    };
    let mut shell = false;
    let mut apply_patch = false;
    for tool in tools {
        match tool {
            ResponseTool::Shell { .. } | ResponseTool::LocalShell { .. } => {
                shell = true;
                *tool = shell_tool();
            }
            ResponseTool::ApplyPatch { .. } => {
                apply_patch = true;
                *tool = apply_patch_tool();
            }
            ResponseTool::ToolSearch {
                description,
                execution,
                parameters,
                ..
            } => normalize_tool_search(description, execution, parameters),
            ResponseTool::Function { .. }
            | ResponseTool::FileSearch { .. }
            | ResponseTool::Computer { .. }
            | ResponseTool::ComputerUsePreview { .. }
            | ResponseTool::WebSearch { .. }
            | ResponseTool::WebSearch20250826 { .. }
            | ResponseTool::WebFetch { .. }
            | ResponseTool::Memory { .. }
            | ResponseTool::XSearch { .. }
            | ResponseTool::CollectionsSearch { .. }
            | ResponseTool::Mcp { .. }
            | ResponseTool::CodeExecution { .. }
            | ResponseTool::CodeInterpreter { .. }
            | ResponseTool::ImageGeneration { .. }
            | ResponseTool::Custom { .. }
            | ResponseTool::Namespace { .. }
            | ResponseTool::ProgrammaticToolCalling { .. }
            | ResponseTool::WebSearchPreview { .. }
            | ResponseTool::WebSearchPreview20250311 { .. } => {}
        }
    }
    normalize_choice(choice, shell, apply_patch);
}

pub(super) fn normalize_history(items: &mut Vec<ResponseItem>) -> Result<(), ChannelError> {
    for item in items {
        let ResponseItem::Typed(typed) = item else {
            continue;
        };
        let replacement = match typed.as_ref() {
            TypedResponseItem::ShellCall {
                action,
                call_id,
                id,
                status,
                ..
            } => Some(function_call(
                id.clone(),
                call_id.clone(),
                shell_arguments(action),
                status.clone(),
            )),
            TypedResponseItem::LocalShellCall {
                action,
                call_id,
                id,
                status,
                ..
            } => Some(function_call(
                Some(id.clone()),
                call_id.clone(),
                json!({
                    "command": action.command.join("\n"),
                    "timeout_ms": action.timeout_ms,
                    "workdir": action.working_directory,
                }),
                Some(status.clone()),
            )),
            TypedResponseItem::ShellCallOutput {
                call_id,
                id,
                output,
                status,
                ..
            } => Some(function_output(
                id.clone(),
                call_id.clone(),
                output
                    .iter()
                    .flat_map(|part| [&part.stdout, &part.stderr])
                    .filter(|text| !text.is_empty())
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n"),
                status.clone(),
            )),
            TypedResponseItem::LocalShellCallOutput {
                id,
                output,
                status,
                rest,
                ..
            } => Some(function_output(
                Some(id.clone()),
                rest.get("call_id")
                    .and_then(Value::as_str)
                    .unwrap_or(id)
                    .to_owned(),
                output.clone(),
                status.clone(),
            )),
            TypedResponseItem::ApplyPatchCall {
                call_id,
                id,
                operation,
                ..
            } => Some(custom_call(
                id.clone(),
                call_id.clone(),
                patch_text(operation)?,
            )),
            TypedResponseItem::ApplyPatchCallOutput {
                call_id,
                id,
                output,
                status,
                ..
            } => Some(custom_output(
                id.clone(),
                call_id.clone(),
                output.clone().ok_or_else(|| {
                    ChannelError::Prepare("apply_patch_call_output.output missing".into())
                })?,
                status.as_str(),
            )),
            _ => None,
        };
        if let Some(replacement) = replacement {
            **typed = replacement;
        }
    }
    Ok(())
}

fn shell_tool() -> ResponseTool {
    let parameters = json!({
        "type":"object",
        "properties": {
            "command":{"type":"string","description":"Shell script to run."},
            "workdir":{"type":"string","description":"Working directory for the command."},
            "timeout_ms":{"type":"number","description":"Maximum command runtime."}
        },
        "required":["command"],
        "additionalProperties":false
    })
    .as_object()
    .expect("schema is an object")
    .clone();
    ResponseTool::Function {
        name: "shell_command".into(),
        parameters: ResponseFunctionParameters::Schema(parameters),
        strict: ResponseFunctionStrict::Value(false),
        defer_loading: None,
        description: Some("Runs a shell command and returns its output.".into()),
        output_schema: None,
        allowed_callers: None,
        rest: Default::default(),
    }
}

fn apply_patch_tool() -> ResponseTool {
    ResponseTool::Custom {
        name: "apply_patch".into(),
        defer_loading: None,
        description: Some(
            "The apply_patch tool can be used to edit files. This is a FREEFORM tool, so do not wrap the patch in JSON."
                .into(),
        ),
        format: Some(CustomToolInputFormat::Grammar(CustomToolGrammarFormat {
            type_: CustomToolGrammarFormatType::Grammar,
            definition: APPLY_PATCH_GRAMMAR.into(),
            syntax: CustomToolGrammarSyntax::Lark,
            rest: Default::default(),
        })),
        allowed_callers: None,
        rest: Default::default(),
    }
}

fn normalize_tool_search(
    description: &mut Option<String>,
    execution: &Option<ToolSearchExecution>,
    parameters: &mut Option<Value>,
) {
    if matches!(execution.as_ref(), Some(ToolSearchExecution::Client)) {
        description
            .get_or_insert_with(|| "Search deferred tools using a regular expression".to_owned());
        parameters.get_or_insert_with(|| {
            json!({
                "type":"object",
                "properties": {
                    "query":{"type":"string","description":"Search query for deferred tools."},
                    "limit":{"type":"number","description":"Maximum tools to return."}
                },
                "required":["query"],
                "additionalProperties":false
            })
        });
    } else {
        *description = None;
        *parameters = None;
    }
}

fn normalize_choice(choice: &mut Option<ResponseToolChoice>, shell: bool, apply_patch: bool) {
    match choice {
        Some(ResponseToolChoice::Function(function))
            if shell && matches!(function.name.as_str(), "bash" | "shell" | "shell_command") =>
        {
            function.name = "shell_command".into();
            function.type_ = FunctionToolChoiceType::Function;
        }
        Some(ResponseToolChoice::Custom(custom))
            if apply_patch
                && matches!(
                    custom.name.as_str(),
                    "str_replace_editor" | "str_replace_based_edit_tool" | "apply_patch"
                ) =>
        {
            custom.name = "apply_patch".into();
        }
        Some(ResponseToolChoice::Unknown(value)) => {
            let name = value.get("name").and_then(Value::as_str);
            if shell && matches!(name, Some("bash" | "shell" | "shell_command")) {
                *choice = Some(ResponseToolChoice::Function(ResponseFunctionToolChoice {
                    type_: FunctionToolChoiceType::Function,
                    name: "shell_command".into(),
                    rest: Default::default(),
                }));
            } else if apply_patch
                && matches!(
                    name,
                    Some("str_replace_editor" | "str_replace_based_edit_tool" | "apply_patch")
                )
            {
                *choice = Some(ResponseToolChoice::Custom(ResponseCustomToolChoice {
                    type_: gproxy_protocol::openai::common::CustomToolChoiceType::Custom,
                    name: "apply_patch".into(),
                    rest: Default::default(),
                }));
            }
        }
        _ => {}
    }
}

pub(in crate::codex) fn shell_arguments(action: &ShellAction) -> Value {
    let mut value = Value::Object(action.rest.clone());
    value["command"] = Value::String(action.commands.join("\n"));
    if let Some(timeout) = action.timeout_ms {
        value["timeout_ms"] = Value::from(timeout);
    }
    if let Some(max_output_length) = action.max_output_length {
        value["max_output_length"] = Value::from(max_output_length);
    }
    value
}

fn function_call(
    id: Option<String>,
    call_id: String,
    arguments: Value,
    status: Option<ResponseItemLifecycleStatus>,
) -> TypedResponseItem {
    TypedResponseItem::FunctionCall {
        arguments: arguments.to_string(),
        call_id,
        name: "shell_command".into(),
        id: id.map(|id| mapped_id(&id, "fc_")),
        caller: None,
        namespace: None,
        status,
        rest: Default::default(),
    }
}

fn function_output(
    id: Option<String>,
    call_id: String,
    output: String,
    status: Option<ResponseItemLifecycleStatus>,
) -> TypedResponseItem {
    TypedResponseItem::FunctionCallOutput {
        call_id,
        output: ResponseOutput::Text(output),
        id,
        caller: None,
        name: None,
        namespace: None,
        status,
        created_by: None,
        rest: Default::default(),
    }
}

fn custom_call(id: Option<String>, call_id: String, input: String) -> TypedResponseItem {
    TypedResponseItem::CustomToolCall {
        call_id,
        input,
        name: "apply_patch".into(),
        id: id.map(|id| mapped_id(&id, "ctc_")),
        caller: None,
        namespace: None,
        rest: Default::default(),
    }
}

fn custom_output(
    id: Option<String>,
    call_id: String,
    output: String,
    status: &str,
) -> TypedResponseItem {
    TypedResponseItem::CustomToolCallOutput {
        call_id,
        output: ResponseOutput::Text(output),
        id,
        caller: None,
        status: Some(if status == "completed" {
            ResponseItemLifecycleStatus::Completed
        } else {
            ResponseItemLifecycleStatus::Unknown(status.into())
        }),
        created_by: None,
        rest: Default::default(),
    }
}

fn patch_text(operation: &ApplyPatchOperation) -> Result<String, ChannelError> {
    let mut patch = String::from("*** Begin Patch\n");
    match operation.type_.as_str() {
        "create_file" => {
            patch.push_str("*** Add File: ");
            patch.push_str(&operation.path);
            patch.push('\n');
            let diff = operation.diff.as_deref().ok_or_else(|| {
                ChannelError::Prepare("create_file apply patch diff missing".into())
            })?;
            for line in diff.lines() {
                patch.push('+');
                patch.push_str(line);
                patch.push('\n');
            }
        }
        "delete_file" => {
            patch.push_str("*** Delete File: ");
            patch.push_str(&operation.path);
            patch.push('\n');
        }
        "update_file" => {
            patch.push_str("*** Update File: ");
            patch.push_str(&operation.path);
            patch.push('\n');
            patch.push_str(operation.diff.as_deref().ok_or_else(|| {
                ChannelError::Prepare("update_file apply patch diff missing".into())
            })?);
            if !patch.ends_with('\n') {
                patch.push('\n');
            }
        }
        value => {
            return Err(ChannelError::Prepare(format!(
                "unsupported apply patch operation `{value}`"
            )));
        }
    }
    patch.push_str("*** End Patch\n");
    Ok(patch)
}

fn mapped_id(value: &str, prefix: &str) -> String {
    if value.starts_with(prefix) {
        return value.into();
    }
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{prefix}{hash:016x}")
}
