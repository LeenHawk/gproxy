use gproxy_protocol::{claude, openai};

use crate::TransformError;

pub(crate) fn shell_action(input: &claude::JsonObject) -> Option<openai::ShellAction> {
    let commands = strings(input, "commands").or_else(|| strings(input, "command"))?;
    Some(openai::ShellAction {
        commands,
        max_output_length: number(input, "max_output_length"),
        timeout_ms: number(input, "timeout_ms").or_else(|| number(input, "timeout")),
        rest: Default::default(),
    })
}

pub(crate) fn patch_operation(input: &claude::JsonObject) -> Option<openai::ApplyPatchOperation> {
    let command = string(input, "command")?;
    let path = string(input, "path")?;
    match command.as_str() {
        "create" => Some(openai::ApplyPatchOperation::CreateFile {
            diff: string(input, "file_text")?,
            path,
            rest: Default::default(),
        }),
        "delete" => Some(openai::ApplyPatchOperation::DeleteFile {
            path,
            rest: Default::default(),
        }),
        "str_replace" => Some(openai::ApplyPatchOperation::UpdateFile {
            diff: replacement_diff(&string(input, "old_str")?, &string(input, "new_str")?),
            path,
            rest: Default::default(),
        }),
        _ => None,
    }
}

pub(crate) fn bash_input(
    action: openai::ShellAction,
    environment: Option<openai::ShellEnvironment>,
) -> Result<Option<claude::JsonObject>, TransformError> {
    if action.commands.is_empty() {
        return Ok(None);
    }
    let mut input = claude::JsonObject::new();
    input.insert("command".into(), action.commands.join("\n").into());
    if let Some(timeout_ms) = action.timeout_ms {
        input.insert("timeout_ms".into(), timeout_ms.into());
    }
    if let Some(max_output_length) = action.max_output_length {
        input.insert("max_output_length".into(), max_output_length.into());
    }
    if let Some(environment) = environment {
        input.insert("environment".into(), serde_json::to_value(environment)?);
    }
    Ok(Some(input))
}

pub(crate) fn local_bash_input(
    action: openai::LocalShellAction,
) -> Result<Option<claude::JsonObject>, TransformError> {
    if action.command.is_empty() {
        return Ok(None);
    }
    let mut input = claude::JsonObject::new();
    input.insert("command".into(), action.command.join("\n").into());
    if !action.env.is_empty() {
        input.insert("env".into(), serde_json::to_value(action.env)?);
    }
    if let Some(timeout_ms) = action.timeout_ms {
        input.insert("timeout_ms".into(), timeout_ms.into());
    }
    if let Some(user) = action.user {
        input.insert("user".into(), user.into());
    }
    if let Some(directory) = action.working_directory {
        input.insert("working_directory".into(), directory.into());
    }
    Ok(Some(input))
}

pub(crate) fn editor_input(operation: openai::ApplyPatchOperation) -> claude::JsonObject {
    match operation {
        openai::ApplyPatchOperation::CreateFile { diff, path, .. } => [
            ("path".into(), path.into()),
            ("command".into(), "create".into()),
            ("file_text".into(), diff.into()),
        ]
        .into_iter()
        .collect(),
        openai::ApplyPatchOperation::DeleteFile { path, .. } => [
            ("path".into(), path.into()),
            ("command".into(), "delete".into()),
        ]
        .into_iter()
        .collect(),
        openai::ApplyPatchOperation::UpdateFile { diff, path, .. } => {
            let (old, new) = replacement_strings(&diff);
            [
                ("path".into(), path.into()),
                ("command".into(), "str_replace".into()),
                ("old_str".into(), old.into()),
                ("new_str".into(), new.into()),
            ]
            .into_iter()
            .collect()
        }
    }
}

pub(crate) fn arguments_object(arguments: &str) -> Result<claude::JsonObject, TransformError> {
    let value: serde_json::Value = serde_json::from_str(arguments)?;
    match value {
        serde_json::Value::Object(object) => Ok(object),
        value => Ok([("value".into(), value)].into_iter().collect()),
    }
}

pub(crate) fn value_object(value: serde_json::Value) -> claude::JsonObject {
    match value {
        serde_json::Value::Object(object) => object,
        value => [("value".into(), value)].into_iter().collect(),
    }
}

fn string(input: &claude::JsonObject, key: &str) -> Option<String> {
    input.get(key)?.as_str().map(ToOwned::to_owned)
}

fn strings(input: &claude::JsonObject, key: &str) -> Option<Vec<String>> {
    input
        .get(key)?
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                .collect()
        })
        .or_else(|| string(input, key).map(|value| vec![value]))
}

fn number(input: &claude::JsonObject, key: &str) -> Option<u32> {
    input
        .get(key)?
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
}

fn replacement_diff(old: &str, new: &str) -> String {
    let mut diff = String::from("@@\n");
    for line in old.lines() {
        diff.push('-');
        diff.push_str(line);
        diff.push('\n');
    }
    for line in new.lines() {
        diff.push('+');
        diff.push_str(line);
        diff.push('\n');
    }
    diff
}

fn replacement_strings(diff: &str) -> (String, String) {
    let mut old = String::new();
    let mut new = String::new();
    for line in diff.lines() {
        if line.starts_with("@@")
            || line.starts_with("***")
            || line.starts_with("---")
            || line.starts_with("+++")
        {
            continue;
        }
        if let Some(line) = line.strip_prefix('-') {
            old.push_str(line);
            old.push('\n');
        } else if let Some(line) = line.strip_prefix('+') {
            new.push_str(line);
            new.push('\n');
        } else {
            let line = line.strip_prefix(' ').unwrap_or(line);
            old.push_str(line);
            old.push('\n');
            new.push_str(line);
            new.push('\n');
        }
    }
    (old, new)
}
