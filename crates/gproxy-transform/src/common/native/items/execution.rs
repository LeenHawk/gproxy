use gproxy_protocol::openai;

use crate::TransformError;
use crate::common::native::shape;

use super::ClaudeCall;

pub(super) fn local_shell(
    id: String,
    action: openai::LocalShellAction,
    call_id: String,
    rest: openai::Rest,
) -> Result<ClaudeCall, TransformError> {
    let fallback = action.clone();
    let (name, input) = match shape::local_bash_input(action)? {
        Some(input) => ("bash", input),
        None => (
            "local_shell",
            shape::value_object(serde_json::to_value(fallback)?),
        ),
    };
    Ok(ClaudeCall {
        id: call_id,
        name: name.into(),
        input,
        item_id: Some(id),
        rest,
    })
}

pub(super) fn shell(
    action: openai::ShellAction,
    call_id: String,
    id: Option<String>,
    environment: Option<openai::ShellEnvironment>,
    rest: openai::Rest,
) -> Result<ClaudeCall, TransformError> {
    let fallback_action = action.clone();
    let fallback_environment = environment.clone();
    let (name, input) = match shape::bash_input(action, environment)? {
        Some(input) => ("bash", input),
        None => {
            let mut input = shape::value_object(serde_json::to_value(fallback_action)?);
            if let Some(environment) = fallback_environment {
                input.insert("environment".into(), serde_json::to_value(environment)?);
            }
            ("shell", input)
        }
    };
    Ok(ClaudeCall {
        id: call_id,
        name: name.into(),
        input,
        item_id: id,
        rest,
    })
}

pub(super) fn apply_patch(
    call_id: String,
    operation: openai::ApplyPatchOperation,
    id: Option<String>,
    rest: openai::Rest,
) -> ClaudeCall {
    ClaudeCall {
        id: call_id,
        name: "str_replace_based_edit_tool".into(),
        input: shape::editor_input(operation),
        item_id: id,
        rest,
    }
}

pub(super) fn computer(
    id: String,
    call_id: String,
    action: Option<openai::ComputerAction>,
    actions: Option<Vec<openai::ComputerAction>>,
    mut rest: openai::Rest,
) -> Result<ClaudeCall, TransformError> {
    let input = if let Some(action) = action {
        shape::value_object(serde_json::to_value(action)?)
    } else if let Some(actions) = actions {
        [("actions".into(), serde_json::to_value(actions)?)]
            .into_iter()
            .collect()
    } else {
        return Err(TransformError::shape(
            "OpenAI computer call",
            "both action and actions are missing",
        ));
    };
    rest.insert("openai_native_tool".into(), "computer_call".into());
    Ok(ClaudeCall {
        id: call_id,
        name: "computer".into(),
        input,
        item_id: Some(id),
        rest,
    })
}
