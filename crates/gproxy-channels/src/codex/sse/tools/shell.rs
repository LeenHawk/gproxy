use gproxy_protocol::openai::generate_content::responses::ShellAction;
use serde_json::{Value, json};

pub(in crate::codex) fn shell_action(input: &str) -> ShellAction {
    let value: Value = serde_json::from_str(input).unwrap_or_else(|_| json!({"command":input}));
    let mut rest = value.as_object().cloned().unwrap_or_default();
    for field in ["command", "commands", "max_output_length", "timeout_ms"] {
        rest.remove(field);
    }
    let commands = value
        .get("commands")
        .and_then(Value::as_array)
        .map(|commands| {
            commands
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .or_else(|| {
            value
                .get("command")
                .and_then(Value::as_str)
                .map(|command| vec![command.into()])
        })
        .unwrap_or_default();
    ShellAction {
        commands,
        max_output_length: value
            .get("max_output_length")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok()),
        timeout_ms: value
            .get("timeout_ms")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok()),
        rest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_action_roundtrip_preserves_workdir_and_rest() {
        let action = ShellAction {
            commands: vec!["pwd".into()],
            max_output_length: Some(4096),
            timeout_ms: Some(1000),
            rest: serde_json::Map::from_iter([
                ("workdir".into(), Value::String("/repo".into())),
                ("future_shell".into(), Value::Bool(true)),
            ]),
        };
        let arguments = crate::codex::shape::tools::shell_arguments(&action);
        let restored = shell_action(&arguments.to_string());
        assert_eq!(restored, action);
    }
}
