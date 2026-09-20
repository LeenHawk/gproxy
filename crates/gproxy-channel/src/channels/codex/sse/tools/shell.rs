use super::super::event::invalid;
use crate::channel::ChannelError;
use gproxy_protocol::wire::openai::responses::ShellAction;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct Arguments {
    command: Option<String>,
    commands: Option<Vec<String>>,
    timeout_ms: Option<i64>,
    max_output_length: Option<i64>,
    #[serde(flatten)]
    rest: serde_json::Map<String, Value>,
}

pub(super) fn shell_action(input: &str) -> Result<ShellAction, ChannelError> {
    // v3 also accepts a raw shell script; malformed JSON objects remain errors.
    let value = if input.trim_start().starts_with('{') {
        serde_json::from_str(input).map_err(invalid)?
    } else {
        json!({"command":input})
    };
    let args: Arguments = serde_json::from_value(value).map_err(invalid)?;
    let commands = args
        .commands
        .or_else(|| args.command.map(|command| vec![command]))
        .ok_or_else(|| invalid("shell arguments contain no command"))?;
    Ok(ShellAction {
        commands,
        timeout_ms: args.timeout_ms.map(Some),
        max_output_length: args.max_output_length.map(Some),
        rest: args.rest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shell_action_roundtrip_preserves_workdir_and_rest() {
        let action = ShellAction {
            commands: vec!["pwd".into()],
            timeout_ms: Some(Some(1000)),
            max_output_length: Some(Some(4096)),
            rest: serde_json::Map::from_iter([
                ("workdir".into(), json!("/repo")),
                ("future_shell".into(), json!(true)),
            ]),
        };
        let args = crate::channels::codex::shape::tools::shell_arguments(&action);
        assert_eq!(shell_action(&args.to_string()).unwrap(), action);
    }
}
